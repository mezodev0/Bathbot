use crate::{
    embeds::{Author, Footer},
    util::constants::SYMBOLS,
};

use std::fmt::Write;

pub struct BGRankingEmbed {
    author: Author,
    description: String,
    footer: Footer,
}

impl BGRankingEmbed {
    pub fn new(
        author_idx: Option<usize>,
        list: Vec<(&String, u32)>,
        idx: usize,
        pages: (usize, usize),
    ) -> Self {
        let len = list
            .iter()
            .fold(0, |max, (user, _)| max.max(user.chars().count()));

        let mut description = String::with_capacity(256);
        description.push_str("```\n");

        for (mut i, (user, score)) in list.into_iter().enumerate() {
            i += idx;

            let _ = writeln!(
                description,
                "{i:>2} {:1} # {user:<len$} => {score}",
                if i <= SYMBOLS.len() {
                    SYMBOLS[i - 1]
                } else {
                    ""
                },
                len = len
            );
        }

        description.push_str("```");
        let mut footer_text = format!("Page {}/{}", pages.0, pages.1);

        if let Some(author_idx) = author_idx {
            let _ = write!(footer_text, " ~ Your rank: {}", author_idx + 1);
        }

        Self {
            author: Author::new("Global leaderboard for correct guesses:"),
            description,
            footer: Footer::new(footer_text),
        }
    }
}

impl_builder!(BGRankingEmbed {
    author,
    description,
    footer,
});
