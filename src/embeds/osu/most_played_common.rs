use std::{cmp::Ordering, fmt::Write};

use command_macros::EmbedData;
use hashbrown::HashMap;
use rosu_v2::prelude::MostPlayedMap;

use crate::{
    pagination::Pages,
    util::{constants::OSU_BASE, CowUtils},
};

#[derive(EmbedData)]
pub struct MostPlayedCommonEmbed {
    description: String,
}

impl MostPlayedCommonEmbed {
    pub fn new(
        name1: &str,
        name2: &str,
        map_counts: &[(u32, usize)],
        maps: &HashMap<u32, ([usize; 2], MostPlayedMap)>,
        pages: &Pages,
    ) -> Self {
        let mut description = String::with_capacity(512);

        for ((map_id, _), i) in map_counts.iter().zip(pages.index + 1..) {
            let ([count1, count2], map) = &maps[map_id];

            let (medal1, medal2) = match count1.cmp(count2) {
                Ordering::Less => ("second", "first"),
                Ordering::Equal => ("first", "first"),
                Ordering::Greater => ("first", "second"),
            };

            let _ = writeln!(
                description,
                "**{i}.** [{title} [{version}]]({OSU_BASE}b/{map_id}) [{stars:.2}★]\n\
                - :{medal1}_place: `{name1}`: **{count1}** :{medal2}_place: `{name2}`: **{count2}**",
                title = map.mapset.title.cow_escape_markdown(),
                version = map.map.version.cow_escape_markdown(),
                stars = map.map.stars,
            );
        }

        description.pop();

        Self { description }
    }
}
