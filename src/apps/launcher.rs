use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle, RoundedRectangle};
use embedded_graphics::text::{Alignment, Text};

use crate::app_sdk::SwipeDirection;
use crate::apps::{launcher_entries, App, AppInput, AppResult, AppState};

const ITEM_H: i32 = 65;
const ITEM_GAP: i32 = 6;
const MARGIN_X: i32 = 20;
const START_Y: i32 = 55;
const SCREEN_W: i32 = 410;

fn menu_colors(state: AppState) -> (Rgb565, Rgb565) {
    match state {
        AppState::Snake => (Rgb565::new(2, 20, 2), Rgb565::GREEN),
        AppState::Game2048 => (Rgb565::new(15, 10, 0), Rgb565::YELLOW),
        AppState::Tetris => (Rgb565::new(0, 10, 15), Rgb565::CYAN),
        AppState::Flappy => (Rgb565::new(15, 12, 0), Rgb565::WHITE),
        AppState::Maze => (Rgb565::new(2, 4, 15), Rgb565::WHITE),
        AppState::Sensor => (Rgb565::new(4, 12, 12), Rgb565::CYAN),
        AppState::Mp3Player => (Rgb565::new(0, 8, 15), Rgb565::CYAN),
        AppState::SmartHome => (Rgb565::new(8, 4, 15), Rgb565::new(20, 10, 31)),
        AppState::Settings => (Rgb565::new(6, 12, 6), Rgb565::WHITE),
        AppState::Watchface | AppState::Launcher => (Rgb565::BLACK, Rgb565::WHITE),
    }
}

pub struct Launcher {
    scroll_offset: i32,
    target_scroll: i32,
}

impl Launcher {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            target_scroll: 0,
        }
    }

    fn step(&mut self, swipe: Option<SwipeDirection>, tap: bool, tap_y: u16) -> Option<AppState> {
        let visible_count = launcher_entries().count() as i32;
        let max_scroll = (visible_count * (ITEM_H + ITEM_GAP) - 400).max(0);

        match swipe {
            Some(SwipeDirection::Up) => {
                self.target_scroll = (self.target_scroll + 120).min(max_scroll);
            }
            Some(SwipeDirection::Down) => {
                self.target_scroll = (self.target_scroll - 120).max(0);
            }
            Some(SwipeDirection::Right) => {
                return Some(AppState::Watchface);
            }
            _ => {}
        }

        let diff = self.target_scroll - self.scroll_offset;
        if diff.abs() > 2 {
            self.scroll_offset += diff / 3;
        } else {
            self.scroll_offset = self.target_scroll;
        }

        if tap {
            let y = tap_y as i32 + self.scroll_offset;
            for (i, item) in launcher_entries().enumerate() {
                let item_y = START_Y + i as i32 * (ITEM_H + ITEM_GAP);
                if y >= item_y && y < item_y + ITEM_H {
                    return Some(item.state);
                }
            }
        }

        None
    }
}

impl App for Launcher {
    fn name(&self) -> &str {
        "Launcher"
    }

    fn setup(&mut self) {}

    fn update(&mut self, input: &AppInput) -> AppResult {
        let selected = self.step(
            input.swipe,
            input.tap,
            input.touch.map(|touch| touch.y).unwrap_or_default(),
        );
        match selected {
            Some(state) => AppResult::Transition(state),
            None => AppResult::Continue,
        }
    }

    fn render<D: DrawTarget<Color = Rgb565>>(&self, d: &mut D) {
        let _ = Rectangle::new(Point::zero(), Size::new(410, 502))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::new(1, 2, 2)))
            .draw(d);

        let title = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
        let _ = Text::with_alignment("APPS", Point::new(205, 35), title, Alignment::Center).draw(d);

        for (i, item) in launcher_entries().enumerate() {
            let y = START_Y + i as i32 * (ITEM_H + ITEM_GAP) - self.scroll_offset;
            if y + ITEM_H < 0 || y > 502 {
                continue;
            }
            let (bg_color, text_color) = menu_colors(item.state);

            let _ = RoundedRectangle::with_equal_corners(
                Rectangle::new(
                    Point::new(MARGIN_X, y),
                    Size::new((SCREEN_W - 2 * MARGIN_X) as u32, ITEM_H as u32),
                ),
                Size::new(12, 12),
            )
            .into_styled(PrimitiveStyle::with_fill(bg_color))
            .draw(d);

            let _ = Rectangle::new(
                Point::new(MARGIN_X, y + 8),
                Size::new(4, (ITEM_H - 16) as u32),
            )
            .into_styled(PrimitiveStyle::with_fill(text_color))
            .draw(d);

            let text_style = MonoTextStyle::new(&FONT_10X20, text_color);
            let _ = Text::with_alignment(
                item.display_name,
                Point::new(SCREEN_W / 2, y + ITEM_H / 2 + 5),
                text_style,
                Alignment::Center,
            )
            .draw(d);
        }
    }
}
