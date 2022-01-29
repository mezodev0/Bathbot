mod medal_count;
mod rarity;
mod user_value;

use std::sync::Arc;

use twilight_model::application::interaction::{
    application_command::CommandOptionValue, ApplicationCommand,
};

use crate::{
    commands::{MyCommand, MyCommandOption},
    custom_client::{
        Badges, LovedMapsets, RankedMapsets, Replays, StandardDeviation, Subscribers, TotalPp,
    },
    BotResult, Context, Error,
};

use self::{
    medal_count::medal_count,
    rarity::rarity,
    user_value::{count, pp},
};

use super::UserValue;

enum OsekaiCommandKind {
    Badges,
    LovedMapsets,
    MedalCount,
    RankedMapsets,
    Rarity,
    Replays,
    StandardDeviation,
    Subscribers,
    TotalPp,
}

impl OsekaiCommandKind {
    async fn slash(command: &mut ApplicationCommand) -> BotResult<Self> {
        command
            .data
            .options
            .pop()
            .and_then(|option| match option.value {
                CommandOptionValue::SubCommand(_) => match option.name.as_str() {
                    "badges" => Some(OsekaiCommandKind::Badges),
                    "loved_mapsets" => Some(OsekaiCommandKind::LovedMapsets),
                    "medal_count" => Some(OsekaiCommandKind::MedalCount),
                    "ranked_mapsets" => Some(OsekaiCommandKind::RankedMapsets),
                    "rarity" => Some(OsekaiCommandKind::Rarity),
                    "replays" => Some(OsekaiCommandKind::Replays),
                    "standard_deviation" => Some(OsekaiCommandKind::StandardDeviation),
                    "subscribers" => Some(OsekaiCommandKind::Subscribers),
                    "total_pp" => Some(OsekaiCommandKind::TotalPp),
                    _ => None,
                },
                _ => None,
            })
            .ok_or(Error::InvalidCommandOptions)
    }
}

pub async fn slash_osekai(ctx: Arc<Context>, mut command: ApplicationCommand) -> BotResult<()> {
    match OsekaiCommandKind::slash(&mut command).await? {
        OsekaiCommandKind::Badges => count::<Badges>(ctx, command).await,
        OsekaiCommandKind::LovedMapsets => count::<LovedMapsets>(ctx, command).await,
        OsekaiCommandKind::MedalCount => medal_count(ctx, command).await,
        OsekaiCommandKind::RankedMapsets => count::<RankedMapsets>(ctx, command).await,
        OsekaiCommandKind::Rarity => rarity(ctx, command).await,
        OsekaiCommandKind::Replays => count::<Replays>(ctx, command).await,
        OsekaiCommandKind::StandardDeviation => pp::<StandardDeviation>(ctx, command).await,
        OsekaiCommandKind::Subscribers => count::<Subscribers>(ctx, command).await,
        OsekaiCommandKind::TotalPp => pp::<TotalPp>(ctx, command).await,
    }
}

pub fn define_osekai() -> MyCommand {
    let badges = MyCommandOption::builder("badges", "Who has the most profile badges?")
        .subcommand(Vec::new());

    let loved_mapsets =
        MyCommandOption::builder("loved_mapsets", "Who created the most loved mapsets?")
            .subcommand(Vec::new());

    let medal_count =
        MyCommandOption::builder("medal_count", "Who has the most medals?").subcommand(Vec::new());

    let ranked_mapsets =
        MyCommandOption::builder("ranked_mapsets", "Who created the most ranked mapsets?")
            .subcommand(Vec::new());

    let rarity =
        MyCommandOption::builder("rarity", "What are the rarest medals?").subcommand(Vec::new());

    let replays = MyCommandOption::builder("replays", "Who has the most replays watched?")
        .subcommand(Vec::new());

    let standard_deviation_description =
        "Who has the highest pp standard deviation across all modes?";

    let standard_deviation_help = "Who has the highest pp [standard deviation](https://en.wikipedia.org/wiki/Standard_deviation) across all modes?";

    let standard_deviation =
        MyCommandOption::builder("standard_deviation", standard_deviation_description)
            .help(standard_deviation_help)
            .subcommand(Vec::new());

    let subscribers_description = "Which mapper has the most subscribers?";

    let subscribers =
        MyCommandOption::builder("subscribers", subscribers_description).subcommand(Vec::new());

    let total_pp_description = "Who has the highest total pp in all modes combined?";

    let total_pp =
        MyCommandOption::builder("total_pp", total_pp_description).subcommand(Vec::new());

    let options = vec![
        badges,
        loved_mapsets,
        medal_count,
        ranked_mapsets,
        rarity,
        replays,
        standard_deviation,
        subscribers,
        total_pp,
    ];

    let help = "Various leaderboard stats. \
        All data is provided by [osekai](https://osekai.net/).";

    MyCommand::new("osekai", "Various leaderboards provided by osekai")
        .help(help)
        .options(options)
}
