use std::iter;

use bitflags::bitflags;
use bytes::Bytes;
use eyre::{Result, WrapErr};
use futures::stream::{FuturesUnordered, TryStreamExt};
use image::{codecs::png::PngEncoder, imageops::FilterType::Lanczos3, ColorType, ImageEncoder};
use plotters::{
    coord::{types::RangedCoordi32, Shift},
    prelude::*,
};
use rosu_v2::prelude::{MonthlyCount, User};
use time::{Date, Month, OffsetDateTime};

use crate::{core::Context, util::Monthly};

bitflags! {
    pub struct ProfileGraphFlags: u8 {
        const BADGES    = 1 << 0;
        const PLAYCOUNT = 1 << 1;
        const REPLAYS   = 1 << 2;
    }
}

impl ProfileGraphFlags {
    fn badges(self) -> bool {
        self.contains(Self::BADGES)
    }

    fn playcount(self) -> bool {
        self.contains(Self::PLAYCOUNT)
    }

    fn replays(self) -> bool {
        self.contains(Self::REPLAYS)
    }
}

impl Default for ProfileGraphFlags {
    #[inline]
    fn default() -> Self {
        Self::all()
    }
}

pub struct ProfileGraphParams<'l> {
    ctx: &'l Context,
    user: &'l mut User,
    w: u32,
    h: u32,
    flags: ProfileGraphFlags,
}

impl<'l> ProfileGraphParams<'l> {
    const W: u32 = 1350;
    const H: u32 = 350;

    pub fn new(ctx: &'l Context, user: &'l mut User) -> Self {
        Self {
            ctx,
            user,
            w: Self::W,
            h: Self::H,
            flags: ProfileGraphFlags::default(),
        }
    }

    pub fn width(mut self, w: u32) -> Self {
        self.w = w;

        self
    }

    pub fn height(mut self, h: u32) -> Self {
        self.h = h;

        self
    }

    pub fn flags(mut self, flags: ProfileGraphFlags) -> Self {
        self.flags = flags;

        self
    }
}

type Area<'b> = DrawingArea<BitMapBackend<'b>, Shift>;
type Chart<'a, 'b> =
    ChartContext<'a, BitMapBackend<'b>, Cartesian2d<Monthly<Date>, RangedCoordi32>>;

pub async fn graphs(params: ProfileGraphParams<'_>) -> Result<Option<Vec<u8>>> {
    let w = params.w;
    let h = params.h;

    let len = (w * h) as usize;
    let mut buf = vec![0; len * 3]; // PIXEL_SIZE = 3

    if !draw(&mut buf, params).await? {
        return Ok(None);
    }

    // Encode buf to png
    let mut png_bytes: Vec<u8> = Vec::with_capacity(len);
    let png_encoder = PngEncoder::new(&mut png_bytes);
    png_encoder.write_image(&buf, w, h, ColorType::Rgb8)?;

    Ok(Some(png_bytes))
}

async fn draw(buf: &mut [u8], params: ProfileGraphParams<'_>) -> Result<bool> {
    let ProfileGraphParams {
        ctx,
        user,
        w,
        h,
        flags,
    } = params;

    let (playcounts, replays) = prepare_monthly_counts(user, flags);

    if (flags.playcount() && playcounts.len() < 2) || (!flags.playcount() && replays.len() < 2) {
        return Ok(false);
    }

    let badges = user.badges.take().unwrap_or_default();

    let canvas = if flags.badges() && !badges.is_empty() {
        // Request all badge images
        let badges: Vec<_> = badges
            .iter()
            .map(|badge| ctx.client().get_badge(&badge.image_url))
            .collect::<FuturesUnordered<_>>()
            .try_collect()
            .await?;

        // Needs to happen after .await since type is not Send
        let root = create_root(buf, w, h)?;

        draw_badges(&badges, root, w, h)?
    } else {
        create_root(buf, w, h)?
    };

    if flags.playcount() && flags.replays() {
        draw_both(&playcounts, &replays, &canvas)?;
    } else if flags.replays() {
        draw_replays(&replays, &canvas)?;
    } else if flags.playcount() {
        draw_playcounts(&playcounts, &canvas)?;
    }

    Ok(true)
}

fn create_root(buf: &mut [u8], w: u32, h: u32) -> Result<Area<'_>> {
    let root = BitMapBackend::with_buffer(buf, (w, h)).into_drawing_area();
    let background = RGBColor(19, 43, 33);
    root.fill(&background)
        .wrap_err("failed to fill background")?;

    Ok(root)
}

fn draw_badges<'a>(badges: &[Bytes], area: Area<'a>, w: u32, h: u32) -> Result<Area<'a>> {
    let max_badges_per_row = 10;
    let margin = 5;
    let inner_margin = 3;

    let badge_count = badges.len() as u32;
    let badge_rows = ((badge_count - 1) / max_badges_per_row) + 1;
    let badge_total_height = (badge_rows * 60).min(h / 2);
    let badge_height = badge_total_height / badge_rows;

    let (top, bottom) = area.split_vertically(badge_total_height);

    let mut rows = Vec::with_capacity(badge_rows as usize);
    let mut last = top;

    for _ in 0..badge_rows {
        let (curr, remain) = last.split_vertically(badge_height);
        rows.push(curr);
        last = remain;
    }

    let badge_width =
        (w - 2 * margin - (max_badges_per_row - 1) * inner_margin) / max_badges_per_row;

    // Draw each row of badges
    for (row, chunk) in badges.chunks(max_badges_per_row as usize).enumerate() {
        let x_offset = (max_badges_per_row - chunk.len() as u32) * badge_width / 2;

        let mut chart_row = ChartBuilder::on(&rows[row])
            .margin(margin)
            .build_cartesian_2d(0..w, 0..badge_height)
            .wrap_err("failed to build badge chart")?;

        chart_row
            .configure_mesh()
            .disable_x_axis()
            .disable_y_axis()
            .disable_x_mesh()
            .disable_y_mesh()
            .draw()
            .wrap_err("failed to draw badge mesh")?;

        for (idx, badge) in chunk.iter().enumerate() {
            let badge_img = image::load_from_memory(badge)
                .wrap_err("failed to get badge from memory")?
                .resize_exact(badge_width, badge_height, Lanczos3);

            let x = x_offset + idx as u32 * badge_width + idx as u32 * inner_margin;
            let y = badge_height;
            let elem: BitMapElement<'_, _> = ((x, y), badge_img).into();

            chart_row
                .draw_series(iter::once(elem))
                .wrap_err("failed to draw badge")?;
        }
    }

    Ok(bottom)
}

const PLAYCOUNTS_AREA_COLOR: RGBColor = RGBColor(0, 116, 193);
const PLAYCOUNTS_BORDER_COLOR: RGBColor = RGBColor(102, 174, 222);

fn draw_playcounts(playcounts: &[MonthlyCount], canvas: &Area<'_>) -> Result<()> {
    let (first, last, max) = first_last_max(playcounts);

    let mut chart = ChartBuilder::on(canvas)
        .margin(9_i32)
        .x_label_area_size(20_i32)
        .y_label_area_size(75_i32)
        .build_cartesian_2d(Monthly(first..last), 0..max)
        .wrap_err("failed to build playcounts chart")?;

    chart
        .configure_mesh()
        .light_line_style(&BLACK.mix(0.0))
        .disable_x_mesh()
        .x_labels(10)
        .x_label_formatter(&|d| format!("{}-{}", d.year(), d.month() as u8 + 1))
        .y_desc("Monthly playcount")
        .label_style(("sans-serif", 20_i32, &WHITE))
        .bold_line_style(&WHITE.mix(0.3))
        .axis_style(RGBColor(7, 18, 14))
        .axis_desc_style(("sans-serif", 20_i32, FontStyle::Bold, &WHITE))
        .draw()
        .wrap_err("failed to draw playcounts mesh")?;

    draw_area(
        &mut chart,
        PLAYCOUNTS_AREA_COLOR,
        0.5,
        PLAYCOUNTS_BORDER_COLOR,
        0.6,
        playcounts,
        "Monthly playcount",
    )
    .wrap_err("failed to draw playcount area")
}

const REPLAYS_AREA_COLOR: RGBColor = RGBColor(0, 246, 193);
const REPLAYS_BORDER_COLOR: RGBColor = RGBColor(40, 246, 205);

fn draw_replays(replays: &[MonthlyCount], canvas: &Area<'_>) -> Result<()> {
    let (first, last, max) = first_last_max(replays);
    let label_area = replay_label_area(max);

    let mut chart = ChartBuilder::on(canvas)
        .margin(9_i32)
        .x_label_area_size(20_i32)
        .y_label_area_size(label_area)
        .build_cartesian_2d(Monthly(first..last), 0..max)
        .wrap_err("failed to build replay chart")?;

    chart
        .configure_mesh()
        .light_line_style(&BLACK.mix(0.0))
        .disable_x_mesh()
        .x_labels(10)
        .x_label_formatter(&|d| format!("{}-{}", d.year(), d.month() as u8 + 1))
        .y_desc("Replays watched")
        .label_style(("sans-serif", 20_i32, &WHITE))
        .bold_line_style(&WHITE.mix(0.3))
        .axis_style(RGBColor(7, 18, 14))
        .axis_desc_style(("sans-serif", 20_i32, FontStyle::Bold, &WHITE))
        .draw()
        .wrap_err("failed to draw replay mesh")?;

    draw_area(
        &mut chart,
        REPLAYS_AREA_COLOR,
        0.2,
        REPLAYS_BORDER_COLOR,
        1.0,
        replays,
        "Replays watched",
    )
    .wrap_err("failed to draw replay area")
}

fn draw_both(
    playcounts: &[MonthlyCount],
    replays: &[MonthlyCount],
    canvas: &Area<'_>,
) -> Result<()> {
    let (left_first, left_last, left_max) = first_last_max(playcounts);
    let (right_first, right_last, right_max) = first_last_max(replays);
    let right_label_area = replay_label_area(right_max);

    let x_min = left_first.min(right_first);
    let x_max = left_last.max(right_last);

    let mut chart = ChartBuilder::on(canvas)
        .margin(9_i32)
        .x_label_area_size(20_i32)
        .y_label_area_size(75_i32)
        .right_y_label_area_size(right_label_area)
        .build_cartesian_2d(Monthly(x_min..x_max), 0..left_max)
        .wrap_err("failed to build dual chart")?
        .set_secondary_coord(Monthly(x_min..x_max), 0..right_max);

    // Mesh and labels
    chart
        .configure_mesh()
        .light_line_style(&BLACK.mix(0.0))
        .disable_x_mesh()
        .x_labels(10)
        .x_label_formatter(&|d| format!("{}-{}", d.year(), d.month() as u8))
        .y_desc("Monthly playcount")
        .label_style(("sans-serif", 20_i32, &WHITE))
        .bold_line_style(&WHITE.mix(0.3))
        .axis_style(RGBColor(7, 18, 14))
        .axis_desc_style(("sans-serif", 20_i32, FontStyle::Bold, &WHITE))
        .draw()
        .wrap_err("failed to draw primary mesh")?;

    chart
        .configure_secondary_axes()
        .y_desc("Replays watched")
        .label_style(("sans-serif", 20_i32, &WHITE))
        .axis_style(RGBColor(7, 18, 14))
        .axis_desc_style(("sans-serif", 20_i32, FontStyle::Bold, &WHITE))
        .draw()
        .wrap_err("failed to draw secondary mesh")?;

    draw_area(
        &mut chart,
        PLAYCOUNTS_AREA_COLOR,
        0.5,
        PLAYCOUNTS_BORDER_COLOR,
        0.6,
        playcounts,
        "Monthly playcount",
    )
    .wrap_err("failed to draw playcounts area")?;

    // Draw replay watched area
    // Can't use `draw_area` since it's for the secondary y-axis
    let iter = replays
        .iter()
        .map(|MonthlyCount { start_date, count }| (*start_date, *count));

    let area_color = REPLAYS_AREA_COLOR;
    let border_color = REPLAYS_BORDER_COLOR;
    let series = AreaSeries::new(iter, 0, area_color.mix(0.2).filled());

    chart
        .draw_secondary_series(series.border_style(border_color.stroke_width(1)))
        .wrap_err("failed to draw replays area")?
        .label("Replays watched")
        .legend(move |(x, y)| {
            PathElement::new(vec![(x, y), (x + 20, y)], border_color.stroke_width(2))
        });

    // Draw circles
    let circles = replays.iter().map(|MonthlyCount { start_date, count }| {
        let style = border_color.stroke_width(1).filled();

        Circle::new((*start_date, *count), 2_i32, style)
    });

    chart
        .draw_secondary_series(circles)
        .wrap_err("failed to draw replays circles")?;

    // Legend
    chart
        .configure_series_labels()
        .background_style(&RGBColor(7, 23, 17))
        .position(SeriesLabelPosition::UpperLeft)
        .legend_area_size(45_i32)
        .label_font(("sans-serif", 20_i32, &WHITE))
        .draw()
        .wrap_err("failed to draw legend")?;

    Ok(())
}

fn draw_area(
    chart: &mut Chart<'_, '_>,
    area_color: RGBColor,
    area_mix: f64,
    border_color: RGBColor,
    border_mix: f64,
    monthly_counts: &[MonthlyCount],
    label: &str,
) -> Result<()> {
    // Draw area
    let iter = monthly_counts
        .iter()
        .map(|MonthlyCount { start_date, count }| (*start_date, *count));

    let series = AreaSeries::new(iter, 0, area_color.mix(area_mix).filled());

    chart
        .draw_series(series.border_style(border_color.stroke_width(1)))
        .wrap_err("failed to draw area")?
        .label(label)
        .legend(move |(x, y)| {
            PathElement::new(vec![(x, y), (x + 20, y)], area_color.stroke_width(2))
        });

    // Draw circles
    let circles = monthly_counts
        .iter()
        .map(move |MonthlyCount { start_date, count }| {
            let style = border_color.mix(border_mix).filled();

            Circle::new((*start_date, *count), 2_i32, style)
        });

    chart
        .draw_series(circles)
        .wrap_err("failed to draw circles")?;

    Ok(())
}

fn replay_label_area(max: i32) -> i32 {
    match max {
        n if n < 10 => 40,
        n if n < 100 => 50,
        n if n < 1000 => 60,
        n if n < 10_000 => 70,
        n if n < 100_000 => 80,
        _ => 90,
    }
}

fn first_last_max(counts: &[MonthlyCount]) -> (Date, Date, i32) {
    let first = counts.first().unwrap().start_date;
    let last = counts.last().unwrap().start_date;
    let max = counts.iter().map(|c| c.count).max();

    (first, last, max.map_or(2, |m| m.max(2)))
}

fn prepare_monthly_counts(
    user: &mut User,
    flags: ProfileGraphFlags,
) -> (Vec<MonthlyCount>, Vec<MonthlyCount>) {
    let mut playcounts = user.monthly_playcounts.take().unwrap_or_default();
    let mut replays = user.replays_watched_counts.take().unwrap_or_default();

    // Spoof missing months
    if flags.playcount() {
        spoof_monthly_counts(&mut playcounts);
    }

    // Spoof missing replays
    if !flags.replays() {
        // nothing to do
    } else if !flags.playcount() {
        let now = OffsetDateTime::now_utc();
        let year = now.year();
        let month = now.month();
        let start_date = Date::from_calendar_date(year, month, 1).unwrap();

        if replays.last().map(|c| c.start_date < start_date) == Some(true) {
            let count = MonthlyCount {
                start_date,
                count: 0,
            };

            replays.push(count);
        }

        spoof_monthly_counts(&mut replays);
    } else {
        // For every month in playcounts, make sure there is one in replays
        for (i, start_date) in playcounts.iter().map(|c| c.start_date).enumerate() {
            let cond = replays.get(i).map(|c| c.start_date == start_date);

            if cond != Some(true) {
                let count = MonthlyCount {
                    start_date,
                    count: 0,
                };

                replays.insert(i, count);
            }
        }
    }

    (playcounts, replays)
}

fn spoof_monthly_counts(counts: &mut Vec<MonthlyCount>) {
    // Fill in months inbetween entries
    let (mut year, mut month) = match counts.as_slice() {
        [] | [_] => return,
        [first, ..] => (first.start_date.year(), first.start_date.month()),
    };

    let mut i = 1;

    while i < counts.len() {
        month = month.next();
        year += (month == Month::January) as i32;
        let date = Date::from_calendar_date(year, month, 1).unwrap();

        if date < counts[i].start_date {
            let count = MonthlyCount {
                start_date: date,
                count: 0,
            };

            counts.insert(i, count);
        }

        i += 1;
    }

    // Pad months at the end
    let now = OffsetDateTime::now_utc().date();
    let mut next = counts[counts.len() - 1].start_date;

    next = next.replace_month(next.month().next()).unwrap();

    if next.month() == Month::January {
        next = next.replace_year(next.year() + 1).unwrap();
    }

    while next < now {
        let count = MonthlyCount {
            start_date: next,
            count: 0,
        };

        counts.push(count);

        next = next.replace_month(next.month().next()).unwrap();

        if next.month() == Month::January {
            next = next.replace_year(next.year() + 1).unwrap();
        }
    }
}
