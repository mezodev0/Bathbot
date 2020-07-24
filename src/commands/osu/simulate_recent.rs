use crate::{
    arguments::{Args, SimulateNameArgs},
    embeds::{EmbedData, SimulateEmbed},
    util::{
        constants::{GENERAL_ISSUE, OSU_API_ISSUE},
        MessageExt,
    },
    BotResult, Context,
};

use rosu::{
    backend::requests::RecentRequest,
    models::{
        ApprovalStatus::{Approved, Loved, Ranked},
        GameMode,
    },
};
use std::sync::Arc;
use tokio::time::{self, Duration};
use twilight::model::channel::Message;

#[allow(clippy::cognitive_complexity)]
async fn simulate_recent_send(mode: GameMode, ctx: Arc<Context>, msg: &Message) -> BotResult<()> {
    let args = match SimulateNameArgs::new(Args::new(msg.content.clone())) {
        Ok(args) => args,
        Err(err_msg) => {
            msg.respond(&ctx, err_msg).await?;
            return Ok(());
        }
    };
    let name = if let Some(name) = args.name.as_ref() {
        name.clone()
    } else {
        let data = ctx.data.read().await;
        let links = data.get::<DiscordLinks>().unwrap();
        match links.get(msg.author.id.as_u64()) {
            Some(name) => name.clone(),
            None => {
                msg.channel_id
                    .say(
                        ctx,
                        "Either specify an osu name or link your discord \
                        to an osu profile via `<link osuname`",
                    )
                    .await?
                    .reaction_delete(ctx, msg.author.id)
                    .await;
                return Ok(());
            }
        }
    };

    // Retrieve the recent score
    let score = {
        let request = RecentRequest::with_username(&name).mode(mode).limit(1);
        let data = ctx.data.read().await;
        let osu = data.get::<Osu>().unwrap();
        let mut scores = match request.queue(osu).await {
            Ok(scores) => scores,
            Err(why) => {
                msg.respond(&ctx, OSU_API_ISSUE).await?;
                return Err(why.into());
            }
        };
        match scores.pop() {
            Some(score) => score,
            None => {
                let content = format!("No recent plays found for user `{}`", name);
                msg.respond(&ctx, content).await?;
                return Ok(());
            }
        }
    };

    // Retrieving the score's beatmap
    let (map_to_db, map) = {
        let data = ctx.data.read().await;
        let mysql = data.get::<MySQL>().unwrap();
        match mysql.get_beatmap(score.beatmap_id.unwrap()).await {
            Ok(map) => (false, map),
            Err(_) => {
                let osu = data.get::<Osu>().unwrap();
                let map = match score.get_beatmap(osu).await {
                    Ok(m) => m,
                    Err(why) => {
                        msg.respond(&ctx, OSU_API_ISSUE).await?;
                        return Err(why.into());
                    }
                };
                (
                    map.approval_status == Ranked
                        || map.approval_status == Loved
                        || map.approval_status == Approved,
                    map,
                )
            }
        }
    };

    // Accumulate all necessary data
    let map_copy = if map_to_db { Some(map.clone()) } else { None };
    let data = match SimulateEmbed::new(Some(score), map, args.into(), ctx).await {
        Ok(data) => data,
        Err(why) => {
            msg.respond(&ctx, GENERAL_ISSUE).await?;
            return Err(why);
        }
    };

    // Creating the embed
    let mut response = msg
        .channel_id
        .send_message(&ctx.http, |m| {
            m.content("Simulated score:").embed(|e| data.build(e))
        })
        .await?;

    // Add map to database if its not in already
    if let Some(map) = map_copy {
        let data = ctx.data.read().await;
        let mysql = data.get::<MySQL>().unwrap();
        if let Err(why) = mysql.insert_beatmap(&map).await {
            warn!("Could not add map of simulaterecent command to DB: {}", why);
        }
    }

    response.clone().reaction_delete(ctx, msg.author.id).await;

    // Minimize embed after delay
    time::delay_for(Duration::from_secs(45)).await;
    if let Err(why) = response.edit(ctx, |m| m.embed(|e| data.minimize(e))).await {
        warn!("Error while minimizing simulate recent msg: {}", why);
    }
    Ok(())
}

#[command]
#[short_desc("Display an unchoked version of user's most recent play")]
#[usage("[username] [+mods] [-a acc%] [-300 #300s] [-100 #100s] [-50 #50s] [-m #misses]")]
#[example("badewanne3 +hr -a 99.3 -300 1422 -m 1")]
#[aliases("sr")]
pub async fn simulaterecent(ctx: Arc<Context>, msg: &Message) -> BotResult<()> {
    simulate_recent_send(GameMode::STD, ctx, msg, args).await
}

#[command]
#[short_desc("Display a perfect play on a user's most recently played mania map")]
#[usage("[username] [+mods] [-s score]")]
#[example("badewanne3 +dt -s 8950000")]
#[aliases("srm")]
pub async fn simulaterecentmania(ctx: Arc<Context>, msg: &Message) -> BotResult<()> {
    simulate_recent_send(GameMode::MNA, ctx, msg, args).await
}
