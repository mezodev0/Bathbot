use std::fmt::Write;

use command_macros::EmbedData;
use rosu_v2::prelude::{GameMode, Score, User};
use twilight_model::channel::embed::EmbedField;

use crate::{
    core::Context,
    custom_client::SnipePlayer,
    embeds::{attachment, osu},
    pp::PpCalculator,
    util::{
        builder::{AuthorBuilder, FooterBuilder},
        constants::OSU_BASE,
        datetime::how_long_ago_dynamic,
        numbers::{with_comma_float, with_comma_int},
        osu::grade_completion_mods,
        CowUtils, ScoreExt,
    },
};

#[derive(EmbedData)]
pub struct PlayerSnipeStatsEmbed {
    description: String,
    thumbnail: String,
    title: &'static str,
    url: String,
    author: AuthorBuilder,
    footer: FooterBuilder,
    image: String,
    fields: Vec<EmbedField>,
}

impl PlayerSnipeStatsEmbed {
    pub async fn new(
        user: User,
        player: SnipePlayer,
        first_score: Option<Score>,
        ctx: &Context,
    ) -> Self {
        let footer_text = format!(
            "{:+} #1{} since last update",
            player.difference,
            if player.difference == 1 { "" } else { "s" }
        );

        let (description, fields) = if player.count_first == 0 {
            ("No national #1s :(".to_owned(), Vec::new())
        } else {
            let mut fields = Vec::with_capacity(7);
            let mut description = String::with_capacity(256);

            let _ = writeln!(
                description,
                "**Total #1s: {}** | ranked: {} | loved: {}",
                player.count_first, player.count_ranked, player.count_loved
            );

            fields![fields {
                "Average PP:", with_comma_float(player.avg_pp).to_string(), true;
                "Average acc:", format!("{:.2}%", player.avg_acc), true;
                "Average stars:", format!("{:.2}★", player.avg_stars), true;
            }];

            if let Some(score) = first_score {
                let map = score.map.as_ref().unwrap();

                let (pp, max_pp, stars) = match PpCalculator::new(ctx, map.map_id).await {
                    Ok(base_calc) => {
                        let mut calc = base_calc.score(&score);

                        let stars = calc.stars();
                        let max_pp = calc.max_pp();

                        let pp = match score.pp {
                            Some(pp) => pp,
                            None => calc.pp() as f32,
                        };

                        (Some(pp), Some(max_pp as f32), stars as f32)
                    }
                    Err(err) => {
                        warn!("{:?}", err.wrap_err("Failed to get pp calculator"));

                        (None, None, 0.0)
                    }
                };

                let pp = osu::get_pp(pp, max_pp);

                // TODO: update formatting
                let value = format!(
                    "**[{map}]({OSU_BASE}b/{id})**\t\
                    {grade}\t[{stars:.2}★]\t{score}\t({acc}%)\t[{combo}]\t\
                    [{pp}]\t {hits}\t{ago}",
                    map = player.oldest_first.unwrap().map.cow_escape_markdown(),
                    id = map.map_id,
                    grade = grade_completion_mods(&score, map),
                    score = with_comma_int(score.score),
                    acc = score.acc(GameMode::Osu),
                    combo = osu::get_combo(&score, map),
                    hits = score.hits_string(GameMode::Osu),
                    ago = how_long_ago_dynamic(&score.ended_at)
                );

                fields![fields { "Oldest national #1:", value, false }];
            }

            let mut count_mods = player.count_mods.unwrap();
            let mut value = String::with_capacity(count_mods.len() * 7);
            count_mods.sort_unstable_by(|(_, c1), (_, c2)| c2.cmp(c1));
            let mut iter = count_mods.into_iter();

            if let Some((first_mods, first_amount)) = iter.next() {
                let _ = write!(value, "`{first_mods}: {first_amount}`");
                let mut idx = 0;

                for (mods, amount) in iter {
                    match idx {
                        2 => {
                            idx = 0;
                            let _ = write!(value, " >\n`{mods}: {amount}`");
                        }
                        _ => {
                            idx += 1;
                            let _ = write!(value, " > `{mods}: {amount}`");
                        }
                    }
                }
            }

            fields![fields { "Most used mods:", value, false }];

            (description, fields)
        };

        let url = format!(
            "https://snipe.huismetbenen.nl/player/{code}/osu/{user_id}",
            code = user.country_code.to_lowercase(),
            user_id = user.user_id,
        );

        Self {
            url,
            fields,
            description,
            footer: FooterBuilder::new(footer_text),
            author: author!(user),
            title: "National #1 statistics",
            image: attachment("stats_graph.png"),
            thumbnail: user.avatar_url,
        }
    }
}
