use std::{borrow::Cow, collections::BTreeMap, fmt::Write};

use command_macros::EmbedData;
use hashbrown::HashMap;
use rosu_v2::prelude::{GameMode, Grade, User, UserStatistics, Username};
use twilight_model::{
    channel::embed::EmbedField,
    id::{marker::UserMarker, Id},
};

use crate::{
    commands::osu::ProfileResult,
    embeds::attachment,
    util::{
        builder::{AuthorBuilder, FooterBuilder},
        datetime::{how_long_ago_text, sec_to_minsec, DATETIME_FORMAT},
        hasher::IntHasher,
        numbers::{round, with_comma_int},
        osu::grade_emote,
        CowUtils, Emote,
    },
};

#[derive(Clone, EmbedData)]
pub struct ProfileEmbed {
    author: AuthorBuilder,
    description: String,
    fields: Vec<EmbedField>,
    footer: FooterBuilder,
    image: String,
    thumbnail: String,
}

impl ProfileEmbed {
    pub fn compact(user: &User, max_pp: f32, discord_id: Option<Id<UserMarker>>) -> Self {
        let stats = user.statistics.as_ref().unwrap();
        let level = stats.level.float();
        let playtime = stats.playtime / 60 / 60;

        let mut description = format!(
            "Accuracy: `{acc:.2}%` • Level: `{level:.2}`\n\
            Playcount: `{playcount}` (`{playtime} hrs`) • {mode}\n\
            Max pp play: `{max_pp:.2}pp`",
            acc = stats.accuracy,
            playcount = with_comma_int(stats.playcount),
            mode = Emote::from(user.mode).text(),
        );

        if let Some(user_id) = discord_id {
            let _ = write!(description, " • <@{user_id}>");
        }

        Self {
            author: author!(user),
            description,
            fields: Vec::new(),
            footer: FooterBuilder::new(footer_text(user)),
            image: attachment("profile_graph.png"),
            thumbnail: user.avatar_url.to_owned(),
        }
    }

    pub fn medium(user: &User, bonus_pp: f32, discord_id: Option<Id<UserMarker>>) -> Self {
        let mode = Emote::from(user.mode).text();

        let description = if let Some(user_id) = discord_id {
            format!("**{mode} __stats for <@{user_id}>:__**")
        } else {
            format!("**{mode} __statistics:__**")
        };

        let footer_text = footer_text(user);
        let stats = user.statistics.as_ref().unwrap();
        let fields = main_fields(user, stats, bonus_pp);

        Self {
            author: author!(user),
            description,
            fields,
            footer: FooterBuilder::new(footer_text),
            image: attachment("profile_graph.png"),
            thumbnail: user.avatar_url.to_owned(),
        }
    }

    pub fn full(
        user: &User,
        profile_result: Option<&ProfileResult>,
        globals_count: &BTreeMap<usize, Cow<'static, str>>,
        own_top_scores: usize,
        discord_id: Option<Id<UserMarker>>,
        mapper_names: &HashMap<u32, Username, IntHasher>,
    ) -> Self {
        let mode = Emote::from(user.mode).text();

        let mut description = if let Some(user_id) = discord_id {
            format!("**{mode} __stats for <@{user_id}>:__**")
        } else {
            format!("**{mode} __statistics:__**")
        };

        let footer_text = footer_text(user);
        let stats = user.statistics.as_ref().unwrap();

        let bonus_pp = profile_result
            .as_ref()
            .map_or(0.0, |result| result.bonus_pp);

        let mut fields = main_fields(user, stats, bonus_pp);

        if let Some(values) = profile_result {
            let mut avg_string = String::with_capacity(256);
            avg_string.push_str("```\n");
            let _ = writeln!(avg_string, "   |   PP   |  Acc  | Combo | Map len");
            let _ = writeln!(avg_string, "---+--------+-------+-------+--------");

            #[allow(clippy::to_string_in_format_args)]
            let _ = writeln!(
                avg_string,
                "Min|{:^8.2}|{:^7}|{:^7}| {:^7}",
                values.pp.min(),
                round(values.acc.min()),
                values.combo.min(),
                sec_to_minsec(values.map_len.min()).to_string()
            );

            #[allow(clippy::to_string_in_format_args)]
            let _ = writeln!(
                avg_string,
                "Avg|{:^8.2}|{:^7}|{:^7}| {:^7}",
                values.pp.avg(),
                round(values.acc.avg()),
                values.combo.avg(),
                sec_to_minsec(values.map_len.avg()).to_string()
            );

            #[allow(clippy::to_string_in_format_args)]
            let _ = writeln!(
                avg_string,
                "Max|{:^8.2}|{:^7}|{:^7}| {:^7}",
                values.pp.max(),
                round(values.acc.max()),
                values.combo.max(),
                sec_to_minsec(values.map_len.max()).to_string()
            );

            avg_string.push_str("```");
            let mut combo = String::from(&values.combo.avg().to_string());

            match values.mode {
                GameMode::Osu | GameMode::Catch => {
                    let _ = write!(combo, "/{}", values.map_combo);
                }
                _ => {}
            }

            let _ = write!(combo, " [{} - {}]", values.combo.min(), values.combo.max());
            fields![fields { "Averages of top 100 scores", avg_string, false }];

            let mult_mods = values.mod_combs_count.is_some();

            if let Some(mod_combs_count) = values.mod_combs_count.as_ref() {
                let len = mod_combs_count.len();
                let mut value = String::with_capacity(len * 14);
                let mut iter = mod_combs_count.iter();
                let (mods, count) = iter.next().unwrap();
                let _ = write!(value, "`{mods} {count}%`");

                for (mods, count) in iter {
                    let _ = write!(value, " > `{mods} {count}%`");
                }

                fields![fields  { "Favourite mod combinations", value, false }];
            }

            fields.reserve_exact(5);
            let len = values.mods_count.len();
            let mut value = String::with_capacity(len * 14);
            let mut iter = values.mods_count.iter();
            let (mods, count) = iter.next().unwrap();
            let _ = write!(value, "`{mods} {count}%`");

            for (mods, count) in iter {
                let _ = write!(value, " > `{mods} {count}%`");
            }

            fields![fields { "Favourite mods", value, false }];
            let len = values.mod_combs_pp.len();
            let mut value = String::with_capacity(len * 15);
            let mut iter = values.mod_combs_pp.iter();
            let (mods, pp) = iter.next().unwrap();
            let _ = write!(value, "`{mods} {pp:.2}pp`");

            for (mods, pp) in iter {
                let _ = write!(value, " > `{mods} {pp:.2}pp`");
            }

            let name = if mult_mods {
                "PP earned with mod combination"
            } else {
                "PP earned with mod"
            };

            fields![fields { name, value, false }];

            let ranked_count = user.ranked_mapset_count.unwrap()
                + user.loved_mapset_count.unwrap()
                + user.guest_mapset_count.unwrap();

            if ranked_count > 0 {
                let mut mapper_stats = String::with_capacity(64);

                let _ = writeln!(
                    mapper_stats,
                    "`Ranked {}` • `Unranked {}` • `Guest: {}`\n\
                    `Loved {}` • `Graveyard {}`",
                    user.ranked_mapset_count.unwrap_or(0),
                    user.pending_mapset_count.unwrap_or(0),
                    user.guest_mapset_count.unwrap_or(0),
                    user.loved_mapset_count.unwrap_or(0),
                    user.graveyard_mapset_count.unwrap_or(0),
                );

                if own_top_scores > 0 {
                    let _ = writeln!(mapper_stats, "Own maps in top scores: {own_top_scores}");
                }

                fields![fields { "Mapsets from player", mapper_stats, false }];
            }

            let len = mapper_names.values().map(|name| name.len() + 12).sum();

            let mut value = String::with_capacity(len);

            let iter = values.mappers.iter().map(|(id, count, pp)| {
                let name = match mapper_names.get(id) {
                    Some(name) => name.cow_escape_markdown(),
                    None => format!("User id {id}").into(),
                };

                (name, count, pp)
            });

            for (name, count, pp) in iter {
                let _ = writeln!(value, "{name}: {pp:.2}pp ({count})");
            }

            fields![fields { "Mappers in top 100", value, true }];

            let count_len = globals_count
                .iter()
                .fold(0, |max, (_, count)| max.max(count.len()));

            let mut count_str = String::with_capacity(64);
            count_str.push_str("```\n");

            for (rank, count) in globals_count {
                let _ = writeln!(count_str, "Top {rank:<2}: {count:>count_len$}",);
            }

            count_str.push_str("```");
            fields![fields { "Global leaderboards", count_str, true }];
        } else {
            description.push_str("\n\n No Top scores");
        }

        Self {
            author: author!(user),
            description,
            fields,
            footer: FooterBuilder::new(footer_text),
            image: attachment("profile_graph.png"),
            thumbnail: user.avatar_url.to_owned(),
        }
    }
}

fn footer_text(user: &User) -> String {
    format!(
        "Joined osu! {} ({})",
        user.join_date.format(DATETIME_FORMAT).unwrap(),
        how_long_ago_text(&user.join_date),
    )
}

fn main_fields(user: &User, stats: &UserStatistics, bonus_pp: f32) -> Vec<EmbedField> {
    let level = stats.level.float();

    let grades_value = format!(
        "{}{} {}{} {}{} {}{} {}{}",
        grade_emote(Grade::XH),
        stats.grade_counts.ssh,
        grade_emote(Grade::X),
        stats.grade_counts.ss,
        grade_emote(Grade::SH),
        stats.grade_counts.sh,
        grade_emote(Grade::S),
        stats.grade_counts.s,
        grade_emote(Grade::A),
        stats.grade_counts.a,
    );

    let playcount_value = format!(
        "{} / {} hrs",
        with_comma_int(stats.playcount),
        stats.playtime / 60 / 60
    );

    fields![
        "Ranked score", with_comma_int(stats.ranked_score).to_string(), true;
        "Accuracy", format!("{:.2}%", stats.accuracy), true;
        "Max combo", with_comma_int(stats.max_combo).to_string(), true;
        "Total score", with_comma_int(stats.total_score).to_string(), true;
        "Level", format!("{:.2}", level), true;
        "Medals", format!("{}", user.medals.as_ref().unwrap().len()), true;
        "Total hits", with_comma_int(stats.total_hits).to_string(), true;
        "Bonus PP", format!("{bonus_pp}pp"), true;
        "Followers", with_comma_int(user.follower_count.unwrap_or(0)).to_string(), true;
        "Grades", grades_value, false;
        "Play count / time", playcount_value, true;
        "Replays watched", with_comma_int(stats.replays_watched).to_string(), true;
    ]
}
