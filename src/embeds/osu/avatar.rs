use crate::{
    embeds::Author,
    util::{constants::OSU_BASE, osu::flag_url},
};

use rosu_v2::model::user::User;

pub struct AvatarEmbed {
    author: Author,
    image: String,
    url: String,
}

impl AvatarEmbed {
    pub fn new(user: User) -> Self {
        let author = Author::new(user.username.into_string())
            .url(format!("{OSU_BASE}u/{}", user.user_id))
            .icon_url(flag_url(user.country_code.as_str()));

        Self {
            author,
            image: user.avatar_url,
            url: format!("{OSU_BASE}u/{}", user.user_id),
        }
    }
}

impl_builder!(AvatarEmbed { author, image, url });
