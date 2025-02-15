use command_macros::pagination;
use rosu_v2::prelude::User;
use twilight_model::channel::embed::Embed;

use crate::{
    commands::osu::MedalEntryList,
    embeds::{EmbedData, MedalsListEmbed},
};

use super::Pages;

#[pagination(per_page = 10, entries = "medals")]
pub struct MedalsListPagination {
    user: User,
    acquired: (usize, usize),
    medals: Vec<MedalEntryList>,
}

impl MedalsListPagination {
    pub fn build_page(&mut self, pages: &Pages) -> Embed {
        let idx = pages.index;
        let limit = self.medals.len().min(idx + pages.per_page);

        MedalsListEmbed::new(&self.user, &self.medals[idx..limit], self.acquired, pages).build()
    }
}
