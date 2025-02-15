#![cfg(feature = "server")]

use std::sync::Arc;

use command_macros::{command, SlashCommand};
use eyre::Result;
use twilight_interactions::command::CreateCommand;

use crate::{
    commands::utility::{config, Config, ConfigLink},
    util::{constants::INVITE_LINK, interaction::InteractionCommand, ChannelExt},
    Context,
};

#[derive(CreateCommand, SlashCommand)]
#[command(
    name = "link",
    help = "Link your discord to an osu! profile.\n\
    To unlink, use the `/config` command.\n\
    To link your discord to a twitch account you can also use the `/config` command."
)]
#[flags(EPHEMERAL)]
/// Link your discord to an osu! profile
pub struct Link;

async fn slash_link(ctx: Arc<Context>, command: InteractionCommand) -> Result<()> {
    let mut args = Config::default();
    args.osu = Some(ConfigLink::Link);

    config(ctx, command, args).await
}

#[command]
#[desc("Deprecated command, use the slash command `/link` instead")]
#[flags(SKIP_DEFER)]
#[group(AllModes)]
async fn prefix_link(ctx: Arc<Context>, msg: &Message) -> Result<()> {
    let content = format!(
        "This command is deprecated and no longer works.\n\
        Use the slash command `/link` instead (no need to specify your osu! name).\n\
        If slash commands are not available in your server, \
        try [re-inviting the bot]({INVITE_LINK})."
    );

    msg.error(&ctx, content).await?;

    Ok(())
}
