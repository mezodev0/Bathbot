mod medal_count;
mod rarity;
mod user_value;

use medal_count::medal_count;
use rarity::rarity;
use user_value::{count, pp};

use super::UserValue;
use crate::{
    commands::SlashCommandBuilder,
    custom_client::{Badges, LovedMapsets, RankedMapsets, Replays, StandardDeviation, TotalPp},
    util::ApplicationCommandExt,
    BotResult, Context, Error,
};

use std::sync::Arc;
use twilight_model::application::{
    command::{Command, CommandOption, OptionsCommandOptionData},
    interaction::{application_command::CommandDataOption, ApplicationCommand},
};

enum OsekaiCommandKind {
    Badges,       // country - user - count
    LovedMapsets, // country - user - count
    MedalCount,
    RankedMapsets, // country - user - count
    Rarity,
    Replays,           // country - user - count
    StandardDeviation, // country - user - value
    TotalPp,           // country - user - value
}

impl OsekaiCommandKind {
    async fn slash(command: &mut ApplicationCommand) -> BotResult<Self> {
        let mut kind = None;

        for option in command.yoink_options() {
            match option {
                CommandDataOption::String { name, .. } => {
                    bail_cmd_option!("osekai", string, name)
                }
                CommandDataOption::Integer { name, .. } => {
                    bail_cmd_option!("osekai", integer, name)
                }
                CommandDataOption::Boolean { name, .. } => {
                    bail_cmd_option!("osekai", boolean, name)
                }
                CommandDataOption::SubCommand { name, .. } => match name.as_str() {
                    "badges" => kind = Some(OsekaiCommandKind::Badges),
                    "loved_mapsets" => kind = Some(OsekaiCommandKind::LovedMapsets),
                    "medal_count" => kind = Some(OsekaiCommandKind::MedalCount),
                    "ranked_mapsets" => kind = Some(OsekaiCommandKind::RankedMapsets),
                    "rarity" => kind = Some(OsekaiCommandKind::Rarity),
                    "replays" => kind = Some(OsekaiCommandKind::Replays),
                    "standard_deviation" => kind = Some(OsekaiCommandKind::StandardDeviation),
                    "total_pp" => kind = Some(OsekaiCommandKind::TotalPp),
                    _ => bail_cmd_option!("osekai", subcommand, name),
                },
            }
        }

        kind.ok_or(Error::InvalidCommandOptions)
    }
}

pub async fn slash_osekai(ctx: Arc<Context>, mut command: ApplicationCommand) -> BotResult<()> {
    match OsekaiCommandKind::slash(&mut command).await? {
        OsekaiCommandKind::Badges => count(ctx, command, Badges).await,
        OsekaiCommandKind::LovedMapsets => count(ctx, command, LovedMapsets).await,
        OsekaiCommandKind::MedalCount => medal_count(ctx, command).await,
        OsekaiCommandKind::RankedMapsets => count(ctx, command, RankedMapsets).await,
        OsekaiCommandKind::Rarity => rarity(ctx, command).await,
        OsekaiCommandKind::Replays => count(ctx, command, Replays).await,
        OsekaiCommandKind::StandardDeviation => pp(ctx, command, StandardDeviation).await,
        OsekaiCommandKind::TotalPp => pp(ctx, command, TotalPp).await,
    }
}

pub fn slash_osekai_command() -> Command {
    let description = "Various leaderboards provided by osekai";

    let options = vec![
        CommandOption::SubCommand(OptionsCommandOptionData {
            description: "Who has the most profile badges?".to_owned(),
            name: "badges".to_owned(),
            options: vec![],
            required: false,
        }),
        CommandOption::SubCommand(OptionsCommandOptionData {
            description: "Who created to most loved mapsets?".to_owned(),
            name: "loved_mapsets".to_owned(),
            options: vec![],
            required: false,
        }),
        CommandOption::SubCommand(OptionsCommandOptionData {
            description: "Who has the most medals?".to_owned(),
            name: "medal_count".to_owned(),
            options: vec![],
            required: false,
        }),
        CommandOption::SubCommand(OptionsCommandOptionData {
            description: "Who created to most ranked mapsets?".to_owned(),
            name: "ranked_mapsets".to_owned(),
            options: vec![],
            required: false,
        }),
        CommandOption::SubCommand(OptionsCommandOptionData {
            description: "What are the rarest medals?".to_owned(),
            name: "rarity".to_owned(),
            options: vec![],
            required: false,
        }),
        CommandOption::SubCommand(OptionsCommandOptionData {
            description: "Who has the most replays watched?".to_owned(),
            name: "replays".to_owned(),
            options: vec![],
            required: false,
        }),
        CommandOption::SubCommand(OptionsCommandOptionData {
            description: "Who has the highest pp standard deviation across all modes?".to_owned(),
            name: "standard_deviation".to_owned(),
            options: vec![],
            required: false,
        }),
        CommandOption::SubCommand(OptionsCommandOptionData {
            description: "Who has the highest total pp in all modes combined?".to_owned(),
            name: "total_pp".to_owned(),
            options: vec![],
            required: false,
        }),
    ];

    SlashCommandBuilder::new("osekai", description)
        .options(options)
        .build()
}
