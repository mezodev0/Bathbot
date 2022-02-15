use crate::{
    commands::osu::WhatIfData,
    embeds::Author,
    util::numbers::{round, with_comma_float, with_comma_int},
};

use rosu_v2::model::user::User;
use std::fmt::Write;

pub struct WhatIfEmbed {
    author: Author,
    description: String,
    thumbnail: String,
    title: String,
}

impl WhatIfEmbed {
    pub fn new(user: User, pp: f32, data: WhatIfData) -> Self {
        let stats = user.statistics.as_ref().unwrap();
        let count = data.count();

        let title = if count <= 1 {
            format!(
                "What if {name} got a new {pp_given}pp score?",
                name = user.username,
                pp_given = round(pp),
            )
        } else {
            format!(
                "What if {name} got {count} new {pp_given}pp scores?",
                name = user.username,
                pp_given = round(pp),
            )
        };

        let description = match data {
            WhatIfData::NonTop100 => {
                format!(
                    "A {pp_given}pp play wouldn't even be in {name}'s top 100 plays.\n\
                     There would not be any significant pp change.",
                    pp_given = round(pp),
                    name = user.username
                )
            }
            WhatIfData::NoScores { count, rank } => {
                let mut d = if count == 1 {
                    format!(
                        "A {pp}pp play would be {name}'s #1 best play.\n\
                        Their pp would change by **+{pp}** to **{pp}pp**",
                        pp = with_comma_float(pp),
                        name = user.username,
                    )
                } else {
                    format!(
                        "A {pp}pp play would be {name}'s #1 best play.\n\
                        Adding {count} of them would change their pp by **{pp:+}** to **{pp}pp**",
                        pp = with_comma_float(pp),
                        name = user.username,
                    )
                };

                if let Some(rank) = rank {
                    let _ = write!(
                        d,
                        "\nand they would reach rank #{}.",
                        with_comma_int(rank.min(stats.global_rank.unwrap_or(0)))
                    );
                } else {
                    d.push('.');
                }

                d
            }
            WhatIfData::Top100 {
                bonus_pp,
                count,
                new_pp,
                new_pos,
                max_pp,
                rank,
            } => {
                let mut d = if count == 1 {
                    format!(
                        "A {pp}pp play would be {name}'s #{num} best play.\n\
                        Their pp would change by **{pp_change:+.2}** to **{new_pp}pp**",
                        pp = round(pp),
                        name = user.username,
                        num = new_pos,
                        pp_change = (new_pp + bonus_pp - stats.pp).max(0.0),
                        new_pp = with_comma_float(new_pp + bonus_pp)
                    )
                } else {
                    format!(
                        "A {pp}pp play would be {name}'s #{num} best play.\n\
                        Adding {count} of them would change their pp by **{pp_change:+.2}** to **{new_pp}pp**",
                        pp = round(pp),
                        name = user.username,
                        num = new_pos,
                        pp_change = (new_pp + bonus_pp - stats.pp).max(0.0),
                        new_pp = with_comma_float(new_pp + bonus_pp)
                    )
                };

                if let Some(rank) = rank {
                    let _ = write!(
                        d,
                        " and they would reach rank #{}.",
                        with_comma_int(rank.min(stats.global_rank.unwrap_or(0)))
                    );
                } else {
                    d.push('.');
                }

                if pp > max_pp * 2.0 {
                    d.push_str("\nThey'd probably also get banned :^)");
                }

                d
            }
        };

        Self {
            author: author!(user),
            description,
            thumbnail: user.avatar_url,
            title,
        }
    }
}

impl_builder!(WhatIfEmbed {
    author,
    description,
    thumbnail,
    title,
});
