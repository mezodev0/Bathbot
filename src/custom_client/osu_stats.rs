use super::deserialize::{
    adjust_mods, str_to_datetime, str_to_f32, str_to_maybe_datetime, str_to_maybe_f32,
};

use crate::{
    commands::osu::OsuStatsScoresOrder,
    util::{osu::ModSelection, CountryCode},
};

use chrono::{DateTime, Utc};
use rosu_v2::prelude::{GameMode, GameMods, Grade, RankStatus, Username};
use serde::{de::Error, Deserialize, Deserializer};
use std::{fmt, str::FromStr};

#[derive(Debug)]
pub struct OsuStatsPlayer {
    pub user_id: u32,
    pub count: u32,
    pub username: Username,
}

#[derive(Deserialize)]
struct Outer {
    #[serde(rename = "userId")]
    user_id: u32,
    count: String,
    #[serde(rename = "osu_user")]
    user: Inner,
}

#[derive(serde::Deserialize)]
pub struct Inner {
    #[serde(rename = "userName")]
    username: Username,
}

impl<'de> Deserialize<'de> for OsuStatsPlayer {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let helper = Outer::deserialize(d)?;

        Ok(OsuStatsPlayer {
            user_id: helper.user_id,
            count: u32::from_str(&helper.count).map_err(D::Error::custom)?,
            username: helper.user.username,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct OsuStatsScore {
    #[serde(rename = "userId")]
    pub user_id: u32,
    pub position: u32,
    #[serde(rename = "rank")]
    pub grade: Grade,
    pub score: u32,
    #[serde(rename = "maxCombo")]
    pub max_combo: u32,
    #[serde(deserialize_with = "str_to_f32")]
    pub accuracy: f32,
    pub count300: u32,
    pub count100: u32,
    pub count50: u32,
    #[serde(rename = "countKatu")]
    pub count_katu: u32,
    #[serde(rename = "countGeki")]
    pub count_geki: u32,
    #[serde(rename = "countMiss")]
    pub count_miss: u32,
    #[serde(rename = "enabledMods", deserialize_with = "adjust_mods")]
    pub enabled_mods: GameMods,
    #[serde(rename = "playDate", deserialize_with = "str_to_datetime")]
    pub date: DateTime<Utc>,
    #[serde(rename = "ppValue")]
    pub pp: Option<f32>,
    #[serde(rename = "beatmap")]
    pub map: OsuStatsMap,
}

#[derive(Debug, Deserialize)]
pub struct OsuStatsMap {
    #[serde(rename = "beatmapId")]
    pub beatmap_id: u32,
    #[serde(rename = "beatmapSetId")]
    pub beatmapset_id: u32,
    #[serde(rename = "approved")]
    pub approval_status: RankStatus,
    #[serde(rename = "lastUpdated", deserialize_with = "str_to_datetime")]
    pub last_updated: DateTime<Utc>,
    #[serde(rename = "approvedDate", deserialize_with = "str_to_maybe_datetime")]
    pub approved_date: Option<DateTime<Utc>>,
    #[serde(rename = "hitLength")]
    pub seconds_drain: u32,
    #[serde(rename = "totalLength")]
    pub seconds_total: u32,
    pub mode: GameMode,
    pub version: String,
    pub artist: String,
    pub title: String,
    pub creator: Username,
    pub bpm: f32,
    pub source: String,
    #[serde(rename = "diffRating", deserialize_with = "str_to_maybe_f32")]
    pub stars: Option<f32>,
    #[serde(rename = "diffSize", deserialize_with = "str_to_f32")]
    pub diff_cs: f32,
    #[serde(rename = "diffOverall", deserialize_with = "str_to_f32")]
    pub diff_od: f32,
    #[serde(rename = "diffApproach", deserialize_with = "str_to_f32")]
    pub diff_ar: f32,
    #[serde(rename = "diffDrain", deserialize_with = "str_to_f32")]
    pub diff_hp: f32,
    #[serde(rename = "maxCombo")]
    pub max_combo: Option<u32>,
}

impl Default for OsuStatsScoresOrder {
    fn default() -> Self {
        Self::Date
    }
}

impl fmt::Display for OsuStatsScoresOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub struct OsuStatsParams {
    pub username: Username,
    pub mode: GameMode,
    pub page: usize,
    pub min_rank: usize,
    pub max_rank: usize,
    pub min_acc: f32,
    pub max_acc: f32,
    pub order: OsuStatsScoresOrder,
    pub mods: Option<ModSelection>,
    pub descending: bool,
}

impl OsuStatsParams {
    pub fn new(username: impl Into<Username>) -> Self {
        Self {
            username: username.into(),
            mode: GameMode::STD,
            page: 1,
            min_rank: 1,
            max_rank: 100,
            min_acc: 0.0,
            max_acc: 100.0,
            order: OsuStatsScoresOrder::default(),
            mods: None,
            descending: true,
        }
    }

    pub fn mode(mut self, mode: GameMode) -> Self {
        self.mode = mode;

        self
    }
}

#[derive(Debug)]
pub struct OsuStatsListParams {
    pub country: Option<CountryCode>,
    pub mode: GameMode,
    pub page: usize,
    pub rank_min: usize,
    pub rank_max: usize,
}
