use std::{fmt::Write, marker::PhantomData, ops::Deref};

use bb8_redis::redis::AsyncCommands;
use eyre::Report;
use rkyv::{
    ser::serializers::{
        AlignedSerializer, AllocScratch, CompositeSerializer, FallbackScratch, HeapScratch,
        SharedSerializeMap,
    },
    AlignedVec, Archive, Deserialize, Infallible, Serialize,
};
use rosu_v2::{
    prelude::{GameMode, OsuError, Rankings, User},
    OsuResult,
};

use crate::{
    commands::osu::UserArgs,
    custom_client::{
        OsekaiBadge, OsekaiMedal, OsekaiRanking, OsuTrackerIdCount, OsuTrackerPpGroup,
        OsuTrackerStats,
    },
};

use super::Context;

pub type ArchivedResult<T, E = Report> = Result<ArchivedBytes<T>, E>;

type Serializer<const N: usize> = CompositeSerializer<
    AlignedSerializer<AlignedVec>,
    FallbackScratch<HeapScratch<N>, AllocScratch>,
    SharedSerializeMap,
>;

#[derive(Copy, Clone)]
pub struct RedisCache<'c> {
    ctx: &'c Context,
}

impl<'c> RedisCache<'c> {
    const USER_SECONDS: usize = 600;
    const OSUTRACKER_STATS_SECONDS: usize = 86_400;
    const OSUTRACKER_PP_GROUP_SECONDS: usize = 86_400;
    const OSUTRACKER_COUNTS_SECONDS: usize = 86_400;
    const MEDALS_SECONDS: usize = 3600;
    const BADGES_SECONDS: usize = 7200;
    const OSEKAI_RANKING: usize = 7200;
    const PP_RANKING_SECONDS: usize = 1800;

    pub fn new(ctx: &'c Context) -> Self {
        Self { ctx }
    }

    pub async fn badges(&self) -> ArchivedResult<Vec<OsekaiBadge>> {
        let key = "osekai_badges";

        let mut conn = match self.ctx.redis_client().get().await {
            Ok(mut conn) => {
                if let Ok(bytes) = conn.get::<_, Vec<u8>>(key).await {
                    if !bytes.is_empty() {
                        self.ctx.stats.inc_cached_badges();
                        trace!("Found badges in cache ({} bytes)", bytes.len());

                        return Ok(ArchivedBytes::new(bytes));
                    }
                }

                conn
            }
            Err(err) => {
                let report = Report::new(err).wrap_err("Failed to get redis connection");
                warn!("{report:?}");

                let badges = self.ctx.client().get_osekai_badges().await?;

                let bytes =
                    rkyv::to_bytes::<_, 200_000>(&badges).expect("failed to serialize badges");

                return Ok(ArchivedBytes::new(bytes));
            }
        };

        let badges = self.ctx.client().get_osekai_badges().await?;
        let bytes = rkyv::to_bytes::<_, 200_000>(&badges).expect("failed to serialize badges");
        let set_fut = conn.set_ex::<_, _, ()>(key, bytes.as_slice(), Self::BADGES_SECONDS);

        if let Err(err) = set_fut.await {
            let report = Report::new(err).wrap_err("Failed to insert bytes into cache");
            warn!("{report:?}");
        }

        Ok(ArchivedBytes::new(bytes))
    }

    pub async fn medals(&self) -> ArchivedResult<Vec<OsekaiMedal>> {
        let key = "osekai_medals";

        let mut conn = match self.ctx.redis_client().get().await {
            Ok(mut conn) => {
                if let Ok(bytes) = conn.get::<_, Vec<u8>>(key).await {
                    if !bytes.is_empty() {
                        self.ctx.stats.inc_cached_medals();
                        trace!("Found medals in cache ({} bytes)", bytes.len());

                        return Ok(ArchivedBytes::new(bytes));
                    }
                }

                conn
            }
            Err(err) => {
                let report = Report::new(err).wrap_err("Failed to get redis connection");
                warn!("{report:?}");

                let medals = self.ctx.client().get_osekai_medals().await?;
                let bytes =
                    rkyv::to_bytes::<_, 80_000>(&medals).expect("failed to serialize medals");

                return Ok(ArchivedBytes::new(bytes));
            }
        };

        let medals = self.ctx.client().get_osekai_medals().await?;
        let bytes = rkyv::to_bytes::<_, 80_000>(&medals).expect("failed to serialize medals");
        let set_fut = conn.set_ex::<_, _, ()>(key, bytes.as_slice(), Self::MEDALS_SECONDS);

        if let Err(err) = set_fut.await {
            let report = Report::new(err).wrap_err("Failed to insert bytes into cache");
            warn!("{report:?}");
        }

        Ok(ArchivedBytes::new(bytes))
    }

    pub async fn osekai_ranking<R>(&self) -> ArchivedResult<Vec<R::Entry>>
    where
        R: OsekaiRanking,
        <R as OsekaiRanking>::Entry: Serialize<Serializer<70_000>>,
    {
        let key = format!("osekai_ranking_{}", R::FORM);

        let mut conn = match self.ctx.redis_client().get().await {
            Ok(mut conn) => {
                if let Ok(bytes) = conn.get::<_, Vec<u8>>(&key).await {
                    if !bytes.is_empty() {
                        self.ctx.stats.inc_cached_osekai_ranking();
                        trace!("Found osekai ranking in cache ({} bytes)", bytes.len());

                        return Ok(ArchivedBytes::new(bytes));
                    }
                }

                conn
            }
            Err(err) => {
                let report = Report::new(err).wrap_err("Failed to get redis connection");
                warn!("{report:?}");

                let ranking = self.ctx.client().get_osekai_ranking::<R>().await?;
                let bytes = rkyv::to_bytes::<_, 70_000>(&ranking)
                    .expect("failed to serialize osekai ranking");

                return Ok(ArchivedBytes::new(bytes));
            }
        };

        let ranking = self.ctx.client().get_osekai_ranking::<R>().await?;
        let bytes =
            rkyv::to_bytes::<_, 70_000>(&ranking).expect("failed to serialize osekai ranking");
        let set_fut = conn.set_ex::<_, _, ()>(key, bytes.as_slice(), Self::OSEKAI_RANKING);

        if let Err(err) = set_fut.await {
            let report = Report::new(err).wrap_err("Failed to insert bytes into cache");
            warn!("{report:?}");
        }

        Ok(ArchivedBytes::new(bytes))
    }

    pub async fn osutracker_pp_group(&self, pp: u32) -> ArchivedResult<OsuTrackerPpGroup> {
        let key = format!("osutracker_pp_group_{pp}");

        let mut conn = match self.ctx.redis_client().get().await {
            Ok(mut conn) => {
                if let Ok(bytes) = conn.get::<_, Vec<u8>>(&key).await {
                    if !bytes.is_empty() {
                        self.ctx.stats.inc_cached_osutracker_pp_group();
                        trace!("Found osutracker pp group in cache ({} bytes)", bytes.len());

                        return Ok(ArchivedBytes::new(bytes));
                    }
                }

                conn
            }
            Err(err) => {
                let report = Report::new(err).wrap_err("Failed to get redis connection");
                warn!("{report:?}");

                let group = self.ctx.client().get_osutracker_pp_group(pp).await?;
                let bytes = rkyv::to_bytes::<_, 7_000>(&group)
                    .expect("failed to serialize osutracker pp groups");

                return Ok(ArchivedBytes::new(bytes));
            }
        };

        let group = self.ctx.client().get_osutracker_pp_group(pp).await?;
        let bytes =
            rkyv::to_bytes::<_, 7_000>(&group).expect("failed to serialize osutracker pp groups");

        let set_fut =
            conn.set_ex::<_, _, ()>(key, bytes.as_slice(), Self::OSUTRACKER_PP_GROUP_SECONDS);

        if let Err(err) = set_fut.await {
            let report = Report::new(err).wrap_err("Failed to insert bytes into cache");
            warn!("{report:?}");
        }

        Ok(ArchivedBytes::new(bytes))
    }

    pub async fn osutracker_stats(&self) -> ArchivedResult<OsuTrackerStats> {
        let key = "osutracker_stats";

        let mut conn = match self.ctx.redis_client().get().await {
            Ok(mut conn) => {
                if let Ok(bytes) = conn.get::<_, Vec<u8>>(key).await {
                    if !bytes.is_empty() {
                        self.ctx.stats.inc_cached_osutracker_stats();
                        trace!("Found osutracker stats in cache ({} bytes)", bytes.len());

                        return Ok(ArchivedBytes::new(bytes));
                    }
                }

                conn
            }
            Err(err) => {
                let report = Report::new(err).wrap_err("Failed to get redis connection");
                warn!("{report:?}");

                let stats = self.ctx.client().get_osutracker_stats().await?;
                let bytes = rkyv::to_bytes::<_, 190_000>(&stats)
                    .expect("failed to serialize osutracker stats");

                return Ok(ArchivedBytes::new(bytes));
            }
        };

        let stats = self.ctx.client().get_osutracker_stats().await?;
        let bytes =
            rkyv::to_bytes::<_, 190_000>(&stats).expect("failed to serialize osutracker stats");
        let set_fut =
            conn.set_ex::<_, _, ()>(key, bytes.as_slice(), Self::OSUTRACKER_STATS_SECONDS);

        if let Err(err) = set_fut.await {
            let report = Report::new(err).wrap_err("Failed to insert bytes into cache");
            warn!("{report:?}");
        }

        Ok(ArchivedBytes::new(bytes))
    }

    pub async fn osutracker_counts(&self) -> ArchivedResult<Vec<OsuTrackerIdCount>> {
        let key = "osutracker_id_counts";

        let mut conn = match self.ctx.redis_client().get().await {
            Ok(mut conn) => {
                if let Ok(bytes) = conn.get::<_, Vec<u8>>(key).await {
                    if !bytes.is_empty() {
                        self.ctx.stats.inc_cached_osutracker_counts();
                        trace!(
                            "Found osutracker id counts in cache ({} bytes)",
                            bytes.len()
                        );

                        return Ok(ArchivedBytes::new(bytes));
                    }
                }

                conn
            }
            Err(err) => {
                let report = Report::new(err).wrap_err("Failed to get redis connection");
                warn!("{report:?}");

                let counts = self.ctx.client().get_osutracker_counts().await?;
                let bytes = rkyv::to_bytes::<_, 330_000>(&counts)
                    .expect("failed to serialize osutracker counts");

                return Ok(ArchivedBytes::new(bytes));
            }
        };

        let counts = self.ctx.client().get_osutracker_counts().await?;
        let bytes =
            rkyv::to_bytes::<_, 330_000>(&counts).expect("failed to serialize osutracker counts");

        let set_fut =
            conn.set_ex::<_, _, ()>(key, bytes.as_slice(), Self::OSUTRACKER_COUNTS_SECONDS);

        if let Err(err) = set_fut.await {
            let report = Report::new(err).wrap_err("Failed to insert bytes into cache");
            warn!("{report:?}");
        }

        Ok(ArchivedBytes::new(bytes))
    }

    pub async fn pp_ranking(
        &self,
        mode: GameMode,
        page: u32,
        country: Option<&str>,
    ) -> ArchivedResult<Rankings, OsuError> {
        let mut key = format!("pp_ranking_{}_{page}", mode as u8);

        if let Some(country) = country {
            let _ = write!(key, "_{country}");
        }

        let mut conn = match self.ctx.redis_client().get().await {
            Ok(mut conn) => {
                if let Ok(bytes) = conn.get::<_, Vec<u8>>(&key).await {
                    if !bytes.is_empty() {
                        self.ctx.stats.inc_cached_pp_ranking();
                        trace!("Found pp ranking in cache ({} bytes)", bytes.len());

                        return Ok(ArchivedBytes::new(bytes));
                    }
                }

                conn
            }
            Err(err) => {
                let report = Report::new(err).wrap_err("Failed to get redis connection");
                warn!("{report:?}");

                let ranking_fut = self.ctx.osu().performance_rankings(mode).page(page);

                let ranking = if let Some(country) = country {
                    ranking_fut.country(country).await?
                } else {
                    ranking_fut.await?
                };

                let bytes =
                    rkyv::to_bytes::<_, 40_000>(&ranking).expect("failed to serialize ranking");

                return Ok(ArchivedBytes::new(bytes));
            }
        };

        let ranking_fut = self.ctx.osu().performance_rankings(mode).page(page);

        let ranking = if let Some(country) = country {
            ranking_fut.country(country).await?
        } else {
            ranking_fut.await?
        };

        let bytes = rkyv::to_bytes::<_, 40_000>(&ranking).expect("failed to serialize ranking");
        let set_fut = conn.set_ex::<_, _, ()>(key, bytes.as_slice(), Self::PP_RANKING_SECONDS);

        if let Err(err) = set_fut.await {
            let report = Report::new(err).wrap_err("Failed to insert bytes into cache");
            warn!("{report:?}");
        }

        Ok(ArchivedBytes::new(bytes))
    }

    pub async fn osu_user(&self, args: &UserArgs<'_>) -> OsuResult<User> {
        let key = format!("__{}_{}", args.name, args.mode as u8);

        let mut conn = match self.ctx.redis_client().get().await {
            Ok(mut conn) => {
                if let Ok(bytes) = conn.get::<_, Vec<u8>>(&key).await {
                    if !bytes.is_empty() {
                        self.ctx.stats.inc_cached_user();
                        trace!(
                            "Found user `{}` in cache ({} bytes)",
                            args.name,
                            bytes.len()
                        );

                        let archived = unsafe { rkyv::archived_root::<User>(&bytes) };
                        let user = archived.deserialize(&mut Infallible).unwrap();

                        return Ok(user);
                    }
                }

                conn
            }
            Err(err) => {
                let report = Report::new(err).wrap_err("Failed to get redis connection");
                warn!("{report:?}");

                let user = match self.ctx.osu().user(args.name).mode(args.mode).await {
                    Ok(user) => user,
                    Err(OsuError::NotFound) => {
                        // Remove stats of unknown/restricted users so they don't appear in the leaderboard
                        if let Err(err) = self.ctx.psql().remove_osu_user_stats(args.name).await {
                            warn!(
                                "{:?}",
                                err.wrap_err("failed to remove stats of unknown user")
                            );
                        }

                        return Err(OsuError::NotFound);
                    }
                    err => return err,
                };

                if let Err(err) = self.ctx.psql().upsert_osu_user(&user, args.mode).await {
                    warn!("{:?}", err.wrap_err("failed to upsert osu user"));
                }

                return Ok(user);
            }
        };

        let mut user = match self.ctx.osu().user(args.name).mode(args.mode).await {
            Ok(user) => user,
            Err(OsuError::NotFound) => {
                // Remove stats of unknown/restricted users so they don't appear in the leaderboard
                if let Err(err) = self.ctx.psql().remove_osu_user_stats(args.name).await {
                    warn!(
                        "{:?}",
                        err.wrap_err("failed to remove stats of unknown user")
                    );
                }

                return Err(OsuError::NotFound);
            }
            err => return err,
        };

        if let Err(err) = self.ctx.psql().upsert_osu_user(&user, args.mode).await {
            warn!("{:?}", err.wrap_err("Failed to upsert osu user"));
        }

        // Remove html user page to reduce overhead
        user.page.take();

        let bytes = rkyv::to_bytes::<_, 13_000>(&user).expect("failed to serialize user");

        // Cache users for 10 minutes and update username in DB
        let set_fut = conn.set_ex::<_, _, ()>(key, bytes.as_slice(), Self::USER_SECONDS);
        let name_update_fut = self
            .ctx
            .psql()
            .upsert_osu_name(user.user_id, &user.username);

        let (set_result, name_update_result) = tokio::join!(set_fut, name_update_fut);

        if let Err(err) = set_result {
            let report = Report::new(err).wrap_err("failed to insert bytes into cache");
            warn!("{report:?}");
        }

        if let Err(err) = name_update_result {
            warn!("{:?}", err.wrap_err("failed to update osu username"));
        }

        Ok(user)
    }
}

pub struct ArchivedBytes<T> {
    bytes: Bytes,
    phantom: PhantomData<T>,
}

impl<T> ArchivedBytes<T> {
    fn new(bytes: impl Into<Bytes>) -> Self {
        Self {
            bytes: bytes.into(),
            phantom: PhantomData,
        }
    }
}

impl<T> ArchivedBytes<T>
where
    T: Archive,
    T::Archived: Deserialize<T, Infallible>,
{
    pub fn get(&self) -> &T::Archived {
        unsafe { rkyv::archived_root::<T>(&self.bytes) }
    }

    pub fn to_inner(&self) -> T {
        self.get().deserialize(&mut Infallible).unwrap()
    }
}

enum Bytes {
    AlignedVec(AlignedVec),
    Vec(Vec<u8>),
}

impl Deref for Bytes {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        match self {
            Bytes::AlignedVec(v) => v.as_slice(),
            Bytes::Vec(v) => v.as_slice(),
        }
    }
}

impl From<AlignedVec> for Bytes {
    #[inline]
    fn from(vec: AlignedVec) -> Self {
        Self::AlignedVec(vec)
    }
}

impl From<Vec<u8>> for Bytes {
    #[inline]
    fn from(vec: Vec<u8>) -> Self {
        Self::Vec(vec)
    }
}
