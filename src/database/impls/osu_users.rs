use std::{cmp::Ordering, collections::BTreeMap};

use eyre::{Result, WrapErr};
use futures::stream::StreamExt;
use hashbrown::HashMap;
use rosu_v2::prelude::{GameMode, User, Username};
use sqlx::Row;
use time::OffsetDateTime;

use crate::{
    commands::osu::UserValue,
    database::{Database, UserStatsColumn, UserValueRaw},
    embeds::RankingEntry,
    util::hasher::IntHasher,
};

const COUNTRY_CODE: &str = "country_code";

type StatsValueResult<T> = Result<Vec<UserValueRaw<T>>>;

impl Database {
    pub async fn approx_rank_from_pp(&self, pp: f32, mode: GameMode) -> Result<u32> {
        let query = sqlx::query!(
            "\
WITH stats AS (\
    SELECT \
        global_rank,\
        pp,\
        last_update \
    FROM \
        osu_user_stats_mode \
    WHERE \
        mode = $1 \
        AND now() - last_update < interval '2 days'\
)\
SELECT \
    * \
FROM ((\
        SELECT \
            global_rank,\
            pp \
        FROM (\
            SELECT \
                * \
            FROM \
                stats \
            WHERE \
                pp >= $2 \
            ORDER BY \
                pp ASC \
            LIMIT 2) AS innerTable \
    ORDER BY \
        last_update DESC \
    LIMIT 1)\
UNION ALL (\
    SELECT \
        global_rank,\
        pp \
    FROM (\
        SELECT \
            * \
        FROM \
            stats \
        WHERE \
            pp <= $2 \
        ORDER BY \
            pp DESC \
        LIMIT 2) AS innerTable \
ORDER BY \
    last_update DESC \
LIMIT 1)) AS neighbors",
            mode as i16,
            pp,
        );

        let mut stream = query.fetch(&self.pool);

        let (higher_rank, higher_pp) = match stream.next().await {
            Some(entry) => {
                let entry = entry.wrap_err("failed to get higher")?;
                let rank = entry.global_rank.unwrap_or(0) as u32;
                let pp = entry.pp.unwrap_or(0.0) as f32;

                (rank, pp)
            }
            None => return Ok(0),
        };

        let lower = stream
            .next()
            .await
            .transpose()
            .wrap_err("failed to get lower")?
            .map(|entry| {
                let rank = entry.global_rank.unwrap_or(0) as u32;
                let pp = entry.pp.unwrap_or(0.0) as f32;

                (rank, pp)
            });

        trace!("PP={pp} => high: ({higher_rank}, {higher_pp}) | low: {lower:?}");

        if let Some((lower_rank, lower_pp)) = lower {
            ensure!(
                (lower_pp..=higher_pp).contains(&pp),
                "{pp}pp is not between {lower_pp} and {higher_pp}"
            );

            if (higher_pp - lower_pp).abs() <= f32::EPSILON {
                Ok(lower_rank.min(higher_rank).saturating_sub(1))
            } else {
                let percent = (higher_pp - pp) / (higher_pp - lower_pp);
                let rank = percent * lower_rank.saturating_sub(higher_rank) as f32;

                Ok(higher_rank + rank as u32)
            }
        } else if higher_pp < pp {
            Ok(higher_rank)
        } else if higher_pp > pp || higher_rank > 0 {
            Ok(higher_rank + 1)
        } else {
            Ok(0)
        }
    }

    pub async fn approx_pp_from_rank(&self, rank: u32, mode: GameMode) -> Result<f32> {
        let query = sqlx::query!(
            "\
WITH stats AS (\
    SELECT \
        global_rank,\
        pp,\
        last_update \
    FROM \
        osu_user_stats_mode \
    WHERE \
        mode = $1 \
        AND now() - last_update < interval '2 days'\
)\
SELECT \
    * \
FROM ((\
        SELECT \
            global_rank,\
            pp \
        FROM (\
            SELECT \
                * \
            FROM \
                stats \
            WHERE \
                global_rank > 0 \
                AND global_rank <= $2 \
            ORDER BY \
                pp ASC \
            LIMIT 2) AS innerTable \
    ORDER BY \
        last_update DESC \
    LIMIT 1)\
UNION ALL (\
    SELECT \
        global_rank,\
        pp \
    FROM (\
        SELECT \
            * \
        FROM \
            stats \
        WHERE \
            global_rank >= $2 \
        ORDER BY \
            pp DESC \
        LIMIT 2) AS innerTable \
ORDER BY \
    last_update DESC \
LIMIT 1)) AS neighbors",
            mode as i16,
            rank as i32,
        );

        let mut stream = query.fetch(&self.pool);

        let (higher_rank, higher_pp) = match stream.next().await {
            Some(entry) => {
                let entry = entry.wrap_err("failed to get higher")?;
                let rank = entry.global_rank.unwrap_or(0) as u32;
                let pp = entry.pp.unwrap_or(0.0) as f32;

                (rank, pp)
            }
            None => return Ok(0.0),
        };

        let lower = stream
            .next()
            .await
            .transpose()
            .wrap_err("failed to get lower")?
            .map(|entry| {
                let rank = entry.global_rank.unwrap_or(0) as u32;
                let pp = entry.pp.unwrap_or(0.0) as f32;

                (rank, pp)
            });

        trace!("Rank {rank} => high: ({higher_rank}, {higher_pp}) | low: {lower:?}");

        if let Some((lower_rank, lower_pp)) = lower {
            ensure!(
                (higher_rank..=lower_rank).contains(&rank),
                "rank {rank} is not between {higher_rank} and {lower_rank}"
            );

            if lower_rank == higher_rank {
                Ok(lower_pp.max(higher_pp) + 0.01)
            } else {
                let percent = (lower_rank - rank) as f32 / (lower_rank - higher_rank) as f32;
                let pp = percent * (higher_pp - lower_pp).max(0.0);

                Ok(lower_pp + pp)
            }
        } else if higher_rank > rank {
            Ok(higher_pp + 0.01)
        } else if higher_rank < rank || higher_pp > 0.0 {
            Ok(higher_pp - 0.01)
        } else {
            Ok(0.0)
        }
    }

    pub async fn remove_osu_user_stats(&self, user: &str) -> Result<()> {
        let query = sqlx::query!(
            "DELETE \
            FROM osu_user_stats S USING osu_user_names N \
            WHERE N.username ILIKE $1 \
                AND S.user_id=N.user_id",
            user
        );

        query
            .execute(&self.pool)
            .await
            .wrap_err("failed to delete stats entry")?;

        let query = sqlx::query!(
            "DELETE \
            FROM osu_user_stats_mode S USING osu_user_names N \
            WHERE N.username ILIKE $1 \
                AND S.user_id=N.user_id",
            user
        );

        query
            .execute(&self.pool)
            .await
            .wrap_err("failed to delete mode stats entry")?;

        Ok(())
    }

    pub async fn get_names_by_ids(&self, ids: &[i32]) -> Result<HashMap<u32, Username, IntHasher>> {
        let query = sqlx::query!(
            "SELECT user_id,username from osu_user_names WHERE user_id=ANY($1)",
            ids
        );

        let mut stream = query.fetch(&self.pool);
        let mut map = HashMap::with_capacity_and_hasher(ids.len(), IntHasher);

        while let Some(row) = stream.next().await.transpose()? {
            map.insert(row.user_id as u32, row.username.into());
        }

        Ok(map)
    }

    /// Be sure wildcards (_, %) are escaped as required!
    pub async fn get_ids_by_names(&self, names: &[String]) -> Result<HashMap<Username, u32>> {
        let query = sqlx::query!(
            "SELECT user_id,username from osu_user_names WHERE username ILIKE ANY($1)",
            names
        );

        let mut stream = query.fetch(&self.pool);
        let mut map = HashMap::with_capacity(names.len());

        while let Some(row) = stream.next().await.transpose()? {
            map.insert(row.username.into(), row.user_id as u32);
        }

        Ok(map)
    }

    pub async fn upsert_osu_user(&self, user: &User, mode: GameMode) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        let name_query = sqlx::query!(
            "INSERT INTO osu_user_names (user_id, username)\
            VALUES ($1,$2) ON CONFLICT (user_id) DO \
            UPDATE \
            SET username=$2",
            user.user_id as i32,
            user.username.as_str(),
        );

        name_query
            .execute(&mut tx)
            .await
            .wrap_err("failed to insert name entry")?;

        let stats_query = sqlx::query!(
            "INSERT INTO osu_user_stats (\
                user_id,\
                country_code,\
                join_date,\
                comment_count,\
                kudosu_total,\
                kudosu_available,\
                forum_post_count,\
                badges, played_maps,\
                followers,\
                graveyard_mapset_count,\
                loved_mapset_count,\
                mapping_followers,\
                previous_usernames_count,\
                ranked_mapset_count,\
                medals\
        )\
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) ON CONFLICT (user_id) DO \
            UPDATE \
            SET country_code=$2,\
            comment_count=$4,\
                kudosu_total=$5,\
                kudosu_available=$6,\
                forum_post_count=$7,\
                badges=$8,\
                played_maps=$9,\
                followers=$10,\
                graveyard_mapset_count=$11,\
                loved_mapset_count=$12,\
                mapping_followers=$13,\
                previous_usernames_count=$14,\
                ranked_mapset_count=$15,\
                medals=$16",
                user.user_id as i32,
                user.country_code.as_str(),
                user.join_date,
                user.comments_count as i32,
                user.kudosu.total,
                user.kudosu.available,
                user.forum_post_count as i32,
                user.badges.as_ref().map_or(0, Vec::len) as i32,
                user.beatmap_playcounts_count.unwrap_or(0) as i32,
                user.follower_count.unwrap_or(0) as i32,
                user.graveyard_mapset_count.unwrap_or(0) as i32,
                user.loved_mapset_count.unwrap_or(0) as i32,
                user.mapping_follower_count.unwrap_or(0) as i32,
                user.previous_usernames.as_ref().map_or(0, Vec::len) as i32,
                user.ranked_mapset_count.unwrap_or(0) as i32,
                user.medals.as_ref().map_or(0, Vec::len) as i32,
                );

        stats_query
            .execute(&mut tx)
            .await
            .wrap_err("failed to insert stats entry")?;

        if let Some(ref stats) = user.statistics {
            let mode_stats_query = sqlx::query!(
                "INSERT INTO osu_user_stats_mode (\
                    user_id,\
                    mode,\
                    accuracy,\
                    pp,\
                    country_rank,\
                    global_rank,\
                    count_ss,\
                    count_ssh,\
                    count_s,\
                    count_sh,\
                    count_a,\
                    level,\
                    max_combo,\
                    playcount,\
                    playtime,\
                    ranked_score,\
                    replays_watched,\
                    total_hits,\
                    total_score,\
                    scores_first\
                )\
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20) ON CONFLICT (user_id,MODE) DO \
                UPDATE \
                SET accuracy=$3,\
                    pp=$4,\
                    country_rank=$5,\
                    global_rank=$6,\
                    count_ss=$7,\
                    count_ssh=$8,\
                    count_s=$9,\
                    count_sh=$10,\
                    count_a=$11,\
                    level=$12,\
                    max_combo=$13,\
                    playcount=$14,\
                    playtime=$15,\
                    ranked_score=$16,\
                    replays_watched=$17,\
                    total_hits=$18,\
                    total_score=$19,\
                    scores_first=$20",
                    user.user_id as i32,
                    mode as i16,
                    stats.accuracy,
                    stats.pp,
                    stats.country_rank.unwrap_or(0) as i32,
                    stats.global_rank.unwrap_or(0) as i32,
                    stats.grade_counts.ss,
                    stats.grade_counts.ssh,
                    stats.grade_counts.s,
                    stats.grade_counts.sh,
                    stats.grade_counts.a,
                    stats.level.current as f32 + stats.level.progress as f32 / 100.0,
                    stats.max_combo as i32,
                    stats.playcount as i32,
                    stats.playtime as i32,
                    stats.ranked_score as i64,
                    stats.replays_watched as i32,
                    stats.total_hits as i64,
                    stats.total_score as i64,
                    user.scores_first_count.unwrap_or(0) as i32,
                    );

            mode_stats_query
                .execute(&mut tx)
                .await
                .wrap_err("failed to insert mode stats entry")?;
        }

        Ok(tx.commit().await?)
    }

    pub async fn get_osu_users_stats(
        &self,
        column: UserStatsColumn,
        discord_ids: &[i64],
    ) -> Result<BTreeMap<usize, RankingEntry>> {
        let column_str = column.as_str();

        match column {
            UserStatsColumn::Badges
            | UserStatsColumn::Comments
            | UserStatsColumn::Followers
            | UserStatsColumn::ForumPosts
            | UserStatsColumn::GraveyardMapsets
            | UserStatsColumn::JoinDate
            | UserStatsColumn::KudosuAvailable
            | UserStatsColumn::KudosuTotal
            | UserStatsColumn::LovedMapsets
            | UserStatsColumn::MappingFollowers
            | UserStatsColumn::Medals
            | UserStatsColumn::PlayedMaps
            | UserStatsColumn::RankedMapsets
            | UserStatsColumn::Usernames => {
                let query = format!(
                    "SELECT username,{column_str},country_code \
                    FROM\
                    (SELECT osu_id \
                       FROM user_configs \
                       WHERE discord_id=ANY($1) \
                         AND osu_id IS NOT NULL) AS configs \
                    JOIN osu_user_names AS names ON configs.osu_id = names.user_id \
                    JOIN\
                    (SELECT user_id,{column_str},country_code \
                       FROM osu_user_stats) AS stats ON names.user_id=stats.user_id"
                );

                if matches!(column, UserStatsColumn::JoinDate) {
                    self.stats_date(&query, column_str, discord_ids)
                        .await
                        .map(|mut values| {
                            values.sort_unstable_by(|v1, v2| {
                                v1.value
                                    .cmp(&v2.value)
                                    .then_with(|| v1.username.cmp(&v2.username))
                            });

                            values.dedup_by(|a, b| a.username == b.username);

                            values
                                .into_iter()
                                .map(|v| RankingEntry {
                                    value: UserValue::Date(v.value),
                                    name: v.username,
                                    country: Some(v.country_code),
                                })
                                .enumerate()
                                .collect()
                        })
                } else {
                    self.stats_u32(&query, column_str, discord_ids)
                        .await
                        .map(|mut values| {
                            values.sort_unstable_by(|v1, v2| {
                                v2.value
                                    .cmp(&v1.value)
                                    .then_with(|| v1.username.cmp(&v2.username))
                            });

                            values.dedup_by(|a, b| a.username == b.username);

                            values
                                .into_iter()
                                .map(|v| RankingEntry {
                                    value: UserValue::Amount(v.value as u64),
                                    name: v.username,
                                    country: Some(v.country_code),
                                })
                                .enumerate()
                                .collect()
                        })
                }
            }
            UserStatsColumn::Accuracy { mode }
            | UserStatsColumn::CountSsh { mode }
            | UserStatsColumn::CountSs { mode }
            | UserStatsColumn::CountSh { mode }
            | UserStatsColumn::CountS { mode }
            | UserStatsColumn::CountA { mode }
            | UserStatsColumn::Level { mode }
            | UserStatsColumn::MaxCombo { mode }
            | UserStatsColumn::Playcount { mode }
            | UserStatsColumn::Playtime { mode }
            | UserStatsColumn::Pp { mode }
            | UserStatsColumn::RankCountry { mode }
            | UserStatsColumn::RankGlobal { mode }
            | UserStatsColumn::Replays { mode }
            | UserStatsColumn::ScoreRanked { mode }
            | UserStatsColumn::ScoreTotal { mode }
            | UserStatsColumn::ScoresFirst { mode }
            | UserStatsColumn::TotalHits { mode } => {
                let query = format!(
                    "SELECT username,{column_str},country_code \
                    FROM\
                    (SELECT osu_id \
                       FROM user_configs \
                       WHERE discord_id=ANY($1) \
                         AND osu_id IS NOT NULL) AS configs \
                    JOIN osu_user_names AS names ON configs.osu_id = names.user_id \
                    JOIN\
                    (SELECT user_id,{column_str} \
                       FROM osu_user_stats_mode \
                       WHERE mode={mode}) AS stats_mode ON names.user_id=stats_mode.user_id \
                    JOIN \
                    (SELECT user_id,\
                              country_code \
                              FROM osu_user_stats) AS stats ON names.user_id=stats.user_id",
                    mode = mode as u8
                );

                match column {
                    UserStatsColumn::Accuracy { .. } => self
                        .stats_f32(&query, column_str, discord_ids)
                        .await
                        .map(|mut values| {
                            values.sort_unstable_by(|v1, v2| {
                                v2.value
                                    .partial_cmp(&v1.value)
                                    .unwrap_or(Ordering::Equal)
                                    .then_with(|| v1.username.cmp(&v2.username))
                            });

                            values.dedup_by(|a, b| a.username == b.username);

                            values
                                .into_iter()
                                .map(|v| RankingEntry {
                                    value: UserValue::Accuracy(v.value),
                                    name: v.username,
                                    country: Some(v.country_code),
                                })
                                .enumerate()
                                .collect()
                        }),
                    UserStatsColumn::Level { .. } => self
                        .stats_f32(&query, column_str, discord_ids)
                        .await
                        .map(|mut values| {
                            values.sort_unstable_by(|v1, v2| {
                                v2.value
                                    .partial_cmp(&v1.value)
                                    .unwrap_or(Ordering::Equal)
                                    .then_with(|| v1.username.cmp(&v2.username))
                            });

                            values.dedup_by(|a, b| a.username == b.username);

                            values
                                .into_iter()
                                .map(|v| RankingEntry {
                                    value: UserValue::Float(v.value),
                                    name: v.username,
                                    country: Some(v.country_code),
                                })
                                .enumerate()
                                .collect()
                        }),
                    UserStatsColumn::Playtime { .. } => self
                        .stats_u32(&query, column_str, discord_ids)
                        .await
                        .map(|mut values| {
                            values.sort_unstable_by(|v1, v2| {
                                v2.value
                                    .cmp(&v1.value)
                                    .then_with(|| v1.username.cmp(&v2.username))
                            });

                            values.dedup_by(|a, b| a.username == b.username);

                            values
                                .into_iter()
                                .map(|v| RankingEntry {
                                    value: UserValue::Playtime(v.value),
                                    name: v.username,
                                    country: Some(v.country_code),
                                })
                                .enumerate()
                                .collect()
                        }),
                    UserStatsColumn::Pp { .. } => self
                        .stats_f32(&query, column_str, discord_ids)
                        .await
                        .map(|mut values| {
                            values.sort_unstable_by(|v1, v2| {
                                v2.value
                                    .partial_cmp(&v1.value)
                                    .unwrap_or(Ordering::Equal)
                                    .then_with(|| v1.username.cmp(&v2.username))
                            });

                            values.dedup_by(|a, b| a.username == b.username);

                            values
                                .into_iter()
                                .map(|v| RankingEntry {
                                    value: UserValue::PpF32(v.value),
                                    name: v.username,
                                    country: Some(v.country_code),
                                })
                                .enumerate()
                                .collect()
                        }),
                    UserStatsColumn::RankCountry { .. } | UserStatsColumn::RankGlobal { .. } => {
                        self.stats_u32(&query, column_str, discord_ids)
                            .await
                            .map(|mut values| {
                                // Filter out inactive players
                                values.retain(|v| v.value != 0);

                                values.sort_unstable_by(|v1, v2| {
                                    v1.value
                                        .cmp(&v2.value)
                                        .then_with(|| v1.username.cmp(&v2.username))
                                });

                                values.dedup_by(|a, b| a.username == b.username);

                                values
                                    .into_iter()
                                    .map(|v| RankingEntry {
                                        value: UserValue::Rank(v.value),
                                        name: v.username,
                                        country: Some(v.country_code),
                                    })
                                    .enumerate()
                                    .collect()
                            })
                    }
                    UserStatsColumn::CountSsh { .. }
                    | UserStatsColumn::CountSs { .. }
                    | UserStatsColumn::CountSh { .. }
                    | UserStatsColumn::CountS { .. }
                    | UserStatsColumn::CountA { .. } => self
                        .stats_i32(&query, column_str, discord_ids)
                        .await
                        .map(|mut values| {
                            values.sort_unstable_by(|v1, v2| {
                                v2.value
                                    .cmp(&v1.value)
                                    .then_with(|| v1.username.cmp(&v2.username))
                            });

                            values.dedup_by(|a, b| a.username == b.username);

                            values
                                .into_iter()
                                .map(|v| RankingEntry {
                                    value: UserValue::AmountWithNegative(v.value as i64),
                                    name: v.username,
                                    country: Some(v.country_code),
                                })
                                .enumerate()
                                .collect()
                        }),
                    UserStatsColumn::MaxCombo { .. }
                    | UserStatsColumn::Playcount { .. }
                    | UserStatsColumn::Replays { .. }
                    | UserStatsColumn::ScoresFirst { .. } => self
                        .stats_u32(&query, column_str, discord_ids)
                        .await
                        .map(|mut values| {
                            values.sort_unstable_by(|v1, v2| {
                                v2.value
                                    .cmp(&v1.value)
                                    .then_with(|| v1.username.cmp(&v2.username))
                            });

                            values.dedup_by(|a, b| a.username == b.username);

                            values
                                .into_iter()
                                .map(|v| RankingEntry {
                                    value: UserValue::Amount(v.value as u64),
                                    name: v.username,
                                    country: Some(v.country_code),
                                })
                                .enumerate()
                                .collect()
                        }),
                    UserStatsColumn::ScoreRanked { .. }
                    | UserStatsColumn::ScoreTotal { .. }
                    | UserStatsColumn::TotalHits { .. } => self
                        .stats_u64(&query, column_str, discord_ids)
                        .await
                        .map(|mut values| {
                            values.sort_unstable_by(|v1, v2| {
                                v2.value
                                    .cmp(&v1.value)
                                    .then_with(|| v1.username.cmp(&v2.username))
                            });

                            values.dedup_by(|a, b| a.username == b.username);

                            values
                                .into_iter()
                                .map(|v| RankingEntry {
                                    value: UserValue::Amount(v.value),
                                    name: v.username,
                                    country: Some(v.country_code),
                                })
                                .enumerate()
                                .collect()
                        }),
                    _ => unreachable!(),
                }
            }
            UserStatsColumn::AverageHits { mode } => {
                let query = sqlx::query!(
                    "SELECT username,total_hits,playcount,country_code \
                    FROM\
                    (SELECT osu_id \
                       FROM user_configs \
                       WHERE discord_id=ANY($1) \
                         AND osu_id IS NOT NULL) AS configs \
                    JOIN osu_user_names AS names ON configs.osu_id = names.user_id \
                    JOIN\
                    (SELECT user_id,total_hits,playcount \
                       FROM osu_user_stats_mode \
                       WHERE mode=$2) AS stats_mode ON names.user_id=stats_mode.user_id \
                    JOIN \
                    (SELECT user_id,\
                        country_code \
                        FROM osu_user_stats) AS stats ON names.user_id=stats.user_id",
                    discord_ids,
                    mode as i16,
                );

                let mut stream = query.fetch(&self.pool);
                let mut users = Vec::with_capacity(discord_ids.len());

                while let Some(row) = stream.next().await.transpose()? {
                    let value = UserValueRaw {
                        username: row.username.into(),
                        country_code: row.country_code.into(),
                        value: (row.total_hits as f32 / row.playcount as f32).max(0.0),
                    };

                    users.push(value);
                }

                users.sort_unstable_by(|v1, v2| {
                    v2.value
                        .partial_cmp(&v1.value)
                        .unwrap_or(Ordering::Equal)
                        .then_with(|| v1.username.cmp(&v2.username))
                });

                users.dedup_by(|a, b| a.username == b.username);

                let values = users
                    .into_iter()
                    .map(|v| RankingEntry {
                        value: UserValue::Float(v.value),
                        name: v.username,
                        country: Some(v.country_code),
                    })
                    .enumerate()
                    .collect();

                Ok(values)
            }
            UserStatsColumn::TotalSs { mode } => {
                let query = sqlx::query!(
                    "SELECT username,count_ssh,count_ss,country_code \
                    FROM\
                    (SELECT osu_id \
                       FROM user_configs \
                       WHERE discord_id=ANY($1) \
                         AND osu_id IS NOT NULL) AS configs \
                    JOIN osu_user_names AS names ON configs.osu_id = names.user_id \
                    JOIN\
                    (SELECT user_id,count_ssh,count_ss \
                       FROM osu_user_stats_mode \
                       WHERE mode=$2) AS stats_mode ON names.user_id=stats_mode.user_id \
                    JOIN \
                    (SELECT user_id,\
                        country_code \
                        FROM osu_user_stats) AS stats ON names.user_id=stats.user_id",
                    discord_ids,
                    mode as i16,
                );

                let mut stream = query.fetch(&self.pool);
                let mut users = Vec::with_capacity(discord_ids.len());

                while let Some(row) = stream.next().await.transpose()? {
                    let value = UserValueRaw {
                        username: row.username.into(),
                        country_code: row.country_code.into(),
                        value: (row.count_ssh + row.count_ss) as u32,
                    };

                    users.push(value);
                }

                users.sort_unstable_by(|v1, v2| {
                    v2.value
                        .cmp(&v1.value)
                        .then_with(|| v1.username.cmp(&v2.username))
                });

                users.dedup_by(|a, b| a.username == b.username);

                let values = users
                    .into_iter()
                    .map(|v| RankingEntry {
                        value: UserValue::Amount(v.value as u64),
                        name: v.username,
                        country: Some(v.country_code),
                    })
                    .enumerate()
                    .collect();

                Ok(values)
            }
            UserStatsColumn::TotalS { mode } => {
                let query = sqlx::query!(
                    "SELECT username,count_sh,count_s,country_code \
                    FROM\
                    (SELECT osu_id \
                       FROM user_configs \
                       WHERE discord_id=ANY($1) \
                         AND osu_id IS NOT NULL) AS configs \
                    JOIN osu_user_names AS names ON configs.osu_id = names.user_id \
                    JOIN\
                    (SELECT user_id,count_sh,count_s \
                       FROM osu_user_stats_mode \
                       WHERE mode=$2) AS stats_mode ON names.user_id=stats_mode.user_id \
                    JOIN \
                    (SELECT user_id,\
                        country_code \
                        FROM osu_user_stats) AS stats ON names.user_id=stats.user_id",
                    discord_ids,
                    mode as i16,
                );

                let mut stream = query.fetch(&self.pool);
                let mut users = Vec::with_capacity(discord_ids.len());

                while let Some(row) = stream.next().await.transpose()? {
                    let value = UserValueRaw {
                        username: row.username.into(),
                        country_code: row.country_code.into(),
                        value: (row.count_sh + row.count_s) as u32,
                    };

                    users.push(value);
                }

                users.sort_unstable_by(|v1, v2| {
                    v2.value
                        .cmp(&v1.value)
                        .then_with(|| v1.username.cmp(&v2.username))
                });

                users.dedup_by(|a, b| a.username == b.username);

                let values = users
                    .into_iter()
                    .map(|v| RankingEntry {
                        value: UserValue::Amount(v.value as u64),
                        name: v.username,
                        country: Some(v.country_code),
                    })
                    .enumerate()
                    .collect();

                Ok(values)
            }
        }
    }

    async fn stats_u32(
        &self,
        query: &str,
        column: &str,
        discord_ids: &[i64],
    ) -> StatsValueResult<u32> {
        let mut stream = sqlx::query(query).bind(discord_ids).fetch(&self.pool);
        let mut users = Vec::with_capacity(discord_ids.len());

        while let Some(row) = stream.next().await.transpose()? {
            let value = UserValueRaw {
                username: row.get::<&str, _>("username").into(),
                country_code: row.get::<&str, _>(COUNTRY_CODE).into(),
                value: row.get::<i32, _>(column) as u32,
            };

            users.push(value);
        }

        Ok(users)
    }

    async fn stats_u64(
        &self,
        query: &str,
        column: &str,
        discord_ids: &[i64],
    ) -> StatsValueResult<u64> {
        let mut stream = sqlx::query(query).bind(discord_ids).fetch(&self.pool);
        let mut users = Vec::with_capacity(discord_ids.len());

        while let Some(row) = stream.next().await.transpose()? {
            let value = UserValueRaw {
                username: row.get::<&str, _>("username").into(),
                country_code: row.get::<&str, _>(COUNTRY_CODE).into(),
                value: row.get::<i64, _>(column) as u64,
            };

            users.push(value);
        }

        Ok(users)
    }

    async fn stats_i32(
        &self,
        query: &str,
        column: &str,
        discord_ids: &[i64],
    ) -> StatsValueResult<i32> {
        let mut stream = sqlx::query(query).bind(discord_ids).fetch(&self.pool);
        let mut users = Vec::with_capacity(discord_ids.len());

        while let Some(row) = stream.next().await.transpose()? {
            let value = UserValueRaw {
                username: row.get::<&str, _>("username").into(),
                country_code: row.get::<&str, _>(COUNTRY_CODE).into(),
                value: row.get(column),
            };

            users.push(value);
        }

        Ok(users)
    }

    async fn stats_f32(
        &self,
        query: &str,
        column: &str,
        discord_ids: &[i64],
    ) -> StatsValueResult<f32> {
        let mut stream = sqlx::query(query).bind(discord_ids).fetch(&self.pool);
        let mut users = Vec::with_capacity(discord_ids.len());

        while let Some(row) = stream.next().await.transpose()? {
            let value = UserValueRaw {
                username: row.get::<&str, _>("username").into(),
                country_code: row.get::<&str, _>(COUNTRY_CODE).into(),
                value: row.get(column),
            };

            users.push(value);
        }

        Ok(users)
    }

    async fn stats_date(
        &self,
        query: &str,
        column: &str,
        discord_ids: &[i64],
    ) -> StatsValueResult<OffsetDateTime> {
        let mut stream = sqlx::query(query).bind(discord_ids).fetch(&self.pool);
        let mut users = Vec::with_capacity(discord_ids.len());

        while let Some(row) = stream.next().await.transpose()? {
            let value = UserValueRaw {
                username: row.get::<&str, _>("username").into(),
                country_code: row.get::<&str, _>(COUNTRY_CODE).into(),
                value: row.get(column),
            };

            users.push(value);
        }

        Ok(users)
    }
}
