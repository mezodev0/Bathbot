use std::{borrow::Cow, cmp::Ordering::Equal, sync::Arc};

use command_macros::command;
use eyre::Report;
use rosu_v2::prelude::{GameMode, OsuError};

use crate::{
    commands::osu::UserArgs,
    core::commands::{prefix::Args, CommandOrigin},
    custom_client::SnipeCountryPlayer as SCP,
    embeds::{CountrySnipeListEmbed, EmbedData},
    pagination::{CountrySnipeListPagination, Pagination},
    util::{
        builder::MessageBuilder,
        constants::{HUISMETBENEN_ISSUE, OSU_API_ISSUE},
        numbers, ChannelExt, CountryCode, CowUtils,
    },
    BotResult, Context,
};

use super::{SnipeCountryList, SnipeCountryListOrder};

#[command]
#[desc("Sort the country's #1 leaderboard")]
#[help(
    "Sort the country's #1 leaderboard.\n\
    To specify a country, you must provide its acronym e.g. `be` \
    or alternatively you can provide `global`.\n\
    To specify an order, you must provide `sort=...` with any of these values:\n\
     - `count` to sort by #1 count\n \
     - `pp` to sort by average pp of #1 scores\n \
     - `stars` to sort by average star rating of #1 scores\n \
     - `weighted` to sort by pp gained only from #1 scores\n\
    If no ordering is specified, it defaults to `count`.\n\
    If no country is specified either, I will take the country of the linked user.\n\
    All data originates from [Mr Helix](https://osu.ppy.sh/users/2330619)'s \
    website [huismetbenen](https://snipe.huismetbenen.nl/)."
)]
#[usage("[country acronym] [sort=count/pp/stars/weighted]")]
#[example("global sort=stars", "fr sort=weighted", "sort=pp")]
#[aliases("csl", "countrysnipeleaderboard", "cslb")]
#[group(Osu)]
async fn prefix_countrysnipelist(
    ctx: Arc<Context>,
    msg: &Message,
    args: Args<'_>,
) -> BotResult<()> {
    match SnipeCountryList::args(args) {
        Ok(args) => country_list(ctx, msg.into(), args).await,
        Err(content) => {
            msg.error(&ctx, content).await?;

            Ok(())
        }
    }
}

pub(super) async fn country_list(
    ctx: Arc<Context>,
    orig: CommandOrigin<'_>,
    args: SnipeCountryList<'_>,
) -> BotResult<()> {
    let author_id = orig.user_id()?;

    // Retrieve author's osu user to check if they're in the list
    let osu_user = match ctx.psql().get_user_osu(author_id).await {
        Ok(Some(osu)) => {
            let name = osu.into_username();
            let user_args = UserArgs::new(name.as_str(), GameMode::STD);

            match ctx.redis().osu_user(&user_args).await {
                Ok(user) => Some(user),
                Err(OsuError::NotFound) => {
                    let content = format!("User `{name}` was not found");

                    return orig.error(&ctx, content).await;
                }
                Err(err) => {
                    let _ = orig.error(&ctx, OSU_API_ISSUE).await;

                    return Err(err.into());
                }
            }
        }
        Ok(None) => None,
        Err(err) => {
            let wrap = "failed to get UserConfig for user {author_id}";
            warn!("{:?}", Report::new(err).wrap_err(wrap));

            None
        }
    };

    let SnipeCountryList { country, sort } = args;

    let country_code = match country {
        Some(country) => match CountryCode::from_name(&country) {
            Some(code) => code,
            None => {
                if country.len() == 2 {
                    CountryCode::from(country)
                } else {
                    let content = format!(
                        "Looks like `{country}` is neither a country name nor a country code"
                    );

                    return orig.error(&ctx, content).await;
                }
            }
        },
        None => match osu_user {
            Some(ref user) => user.country_code.as_str().into(),
            None => {
                let content = "Since you're not linked, you must specify a country (code)";

                return orig.error(&ctx, content).await;
            }
        },
    };

    // Check if huisemetbenen supports the country
    if !country_code.snipe_supported(&ctx) {
        let content = format!("The country code `{country_code}` is not supported :(",);

        return orig.error(&ctx, content).await;
    }

    // Request players
    let mut players = match ctx.client().get_snipe_country(&country_code).await {
        Ok(players) => players,
        Err(why) => {
            let _ = orig.error(&ctx, HUISMETBENEN_ISSUE).await;

            return Err(why.into());
        }
    };

    // Sort players
    let sort = sort.unwrap_or_default();

    let sorter = match sort {
        SnipeCountryListOrder::Count => |p1: &SCP, p2: &SCP| p2.count_first.cmp(&p1.count_first),
        SnipeCountryListOrder::Pp => {
            |p1: &SCP, p2: &SCP| p2.avg_pp.partial_cmp(&p1.avg_pp).unwrap_or(Equal)
        }
        SnipeCountryListOrder::Stars => {
            |p1: &SCP, p2: &SCP| p2.avg_sr.partial_cmp(&p1.avg_sr).unwrap_or(Equal)
        }
        SnipeCountryListOrder::WeightedPp => {
            |p1: &SCP, p2: &SCP| p2.pp.partial_cmp(&p1.pp).unwrap_or(Equal)
        }
    };

    players.sort_unstable_by(sorter);

    // Try to find author in list
    let author_idx = osu_user.as_ref().and_then(|user| {
        players
            .iter()
            .position(|player| player.username == user.username)
    });

    // Enumerate players
    let players: Vec<_> = players
        .into_iter()
        .enumerate()
        .map(|(idx, player)| (idx + 1, player))
        .collect();

    // Prepare embed
    let pages = numbers::div_euclid(10, players.len());
    let init_players = players.iter().take(10);

    let country = ctx
        .get_country(country_code.as_ref())
        .map(|name| (name, country_code.as_ref().into()));

    let embed_data =
        CountrySnipeListEmbed::new(country.as_ref(), sort, init_players, author_idx, (1, pages));

    // Creating the embed
    let embed = embed_data.into_builder().build();
    let builder = MessageBuilder::new().embed(embed);
    let response = orig.create_message(&ctx, &builder).await?.model().await?;

    // Pagination
    let pagination = CountrySnipeListPagination::new(response, players, country, sort, author_idx);
    let owner = author_id;

    tokio::spawn(async move {
        if let Err(err) = pagination.start(&ctx, owner, 60).await {
            warn!("{:?}", Report::new(err));
        }
    });

    Ok(())
}

impl<'m> SnipeCountryList<'m> {
    fn args(args: Args<'m>) -> Result<Self, Cow<'static, str>> {
        let mut country = None;
        let mut sort = None;

        for arg in args.take(2).map(CowUtils::cow_to_ascii_lowercase) {
            if let Some(idx) = arg.find('=').filter(|&i| i > 0) {
                let key = &arg[..idx];
                let value = arg[idx + 1..].trim_end();

                match key {
                    "sort" => {
                        sort = match value {
                            "count" => Some(SnipeCountryListOrder::Count),
                            "pp" => Some(SnipeCountryListOrder::Pp),
                            "stars" => Some(SnipeCountryListOrder::Stars),
                            "weighted" | "weightedpp" => Some(SnipeCountryListOrder::WeightedPp),
                            _ => {
                                let content = "Failed to parse `sort`. \
                                    Must be either `count`, `pp`, `stars`, or `weighted`.";

                                return Err(content.into());
                            }
                        };
                    }
                    _ => {
                        let content =
                            format!("Unrecognized option `{key}`.\nAvailable options are: `sort`.");

                        return Err(content.into());
                    }
                }
            } else {
                country = Some(arg);
            }
        }

        Ok(Self { country, sort })
    }
}
