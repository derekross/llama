//! The Winamp 2.x skin geometry.
//!
//! Every classic skin was cut against these rectangles, so they are a spec,
//! not a design choice: a wrong number here slices a button on every skin at
//! once. Values follow the classic skinning format as documented by Webamp
//! (`skinSprites.ts`). All coordinates are unscaled skin pixels.

/// A rectangle in a sprite sheet, or a placement in a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct R {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

pub const fn r(x: i32, y: i32, w: i32, h: i32) -> R {
    R { x, y, w, h }
}

impl R {
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
}

/// A point where a sprite is drawn in its window.
pub type At = (i32, i32);

pub const MAIN_W: i32 = 275;
pub const MAIN_H: i32 = 116;
pub const SHADE_H: i32 = 14;
pub const EQ_H: i32 = 116;

// ---- main.bmp ------------------------------------------------------------
pub const MAIN_BG: R = r(0, 0, 275, 116);

// ---- titlebar.bmp --------------------------------------------------------
pub const TITLE: R = r(27, 15, 275, 14);
pub const TITLE_ACTIVE: R = r(27, 0, 275, 14);
pub const SHADE_BG: R = r(27, 42, 275, 14);
pub const SHADE_BG_ACTIVE: R = r(27, 29, 275, 14);
pub const SHADE_POS_BG: R = r(0, 36, 17, 7);
pub const SHADE_POS_THUMB: R = r(20, 36, 3, 7);

/// A two-state button: its released and pressed sprites, and where it sits.
#[derive(Clone, Copy, Debug)]
pub struct Button {
    pub up: R,
    pub down: R,
    pub at: At,
}

impl Button {
    pub fn hit(&self) -> R {
        r(self.at.0, self.at.1, self.up.w, self.up.h)
    }
}

pub const BTN_OPTIONS: Button = Button {
    up: r(0, 0, 9, 9),
    down: r(0, 9, 9, 9),
    at: (6, 3),
};
pub const BTN_MINIMIZE: Button = Button {
    up: r(9, 0, 9, 9),
    down: r(9, 9, 9, 9),
    at: (244, 3),
};
pub const BTN_SHADE: Button = Button {
    up: r(0, 18, 9, 9),
    down: r(9, 18, 9, 9),
    at: (254, 3),
};
pub const BTN_UNSHADE: Button = Button {
    up: r(0, 27, 9, 9),
    down: r(9, 27, 9, 9),
    at: (254, 3),
};
pub const BTN_CLOSE: Button = Button {
    up: r(18, 0, 9, 9),
    down: r(18, 9, 9, 9),
    at: (264, 3),
};

/// Clutterbar: O A I D V, stacked down the left edge.
pub const CLUTTER_AT: At = (10, 22);
pub const CLUTTER_BG: R = r(304, 0, 8, 43);
/// (pressed sprite, offset inside the bar) for O, A, I, D, V.
pub const CLUTTER_BUTTONS: [(R, i32); 5] = [
    (r(304, 47, 8, 8), 3),
    (r(312, 55, 8, 7), 11),
    (r(320, 62, 8, 7), 18),
    (r(328, 69, 8, 8), 25),
    (r(336, 77, 8, 7), 33),
];

// ---- cbuttons.bmp --------------------------------------------------------
pub const TRANSPORT: [Button; 6] = [
    Button {
        up: r(0, 0, 23, 18),
        down: r(0, 18, 23, 18),
        at: (16, 88),
    },
    Button {
        up: r(23, 0, 23, 18),
        down: r(23, 18, 23, 18),
        at: (39, 88),
    },
    Button {
        up: r(46, 0, 23, 18),
        down: r(46, 18, 23, 18),
        at: (62, 88),
    },
    Button {
        up: r(69, 0, 23, 18),
        down: r(69, 18, 23, 18),
        at: (85, 88),
    },
    Button {
        up: r(92, 0, 22, 18),
        down: r(92, 18, 22, 18),
        at: (108, 88),
    },
    Button {
        up: r(114, 0, 22, 16),
        down: r(114, 16, 22, 16),
        at: (136, 89),
    },
];

// ---- numbers.bmp / nums_ex.bmp -------------------------------------------
pub const DIGIT_W: i32 = 9;
pub const DIGIT_H: i32 = 13;
pub const DIGITS_AT: [At; 4] = [(48, 26), (60, 26), (78, 26), (90, 26)];
pub const TIME_HIT: R = r(36, 26, 63, 13);
/// numbers.bmp has no minus glyph; Winamp drew it from a 5x1 strip.
pub const MINUS: R = r(20, 6, 5, 1);
pub const MINUS_AT: At = (39, 32);
/// nums_ex.bmp carries a full-size minus at cell 11.
pub const MINUS_EX: R = r(99, 0, 9, 13);
pub const MINUS_EX_AT: At = (36, 26);

pub fn digit(n: u32) -> R {
    r(n as i32 * DIGIT_W, 0, DIGIT_W, DIGIT_H)
}

// ---- playpaus.bmp --------------------------------------------------------
pub const PLAYING: R = r(0, 0, 9, 9);
pub const PAUSED: R = r(9, 0, 9, 9);
pub const STOPPED: R = r(18, 0, 9, 9);
pub const WORKING: R = r(39, 0, 3, 9);
pub const NOT_WORKING: R = r(36, 0, 3, 9);
pub const STATUS_AT: At = (26, 28);
pub const WORKING_AT: At = (24, 28);

// ---- monoster.bmp --------------------------------------------------------
pub const STEREO_ON: R = r(0, 0, 29, 12);
pub const STEREO_OFF: R = r(0, 12, 29, 12);
pub const MONO_ON: R = r(29, 0, 27, 12);
pub const MONO_OFF: R = r(29, 12, 27, 12);
pub const MONO_AT: At = (212, 41);
pub const STEREO_AT: At = (239, 41);

// ---- text areas (text.bmp glyphs) ----------------------------------------
pub const MARQUEE: R = r(111, 27, 154, 6);
pub const KBPS: R = r(111, 43, 15, 6);
pub const KHZ: R = r(156, 43, 10, 6);
pub const VIS: R = r(24, 43, 76, 16);
pub const VIS_SHADE: R = r(79, 5, 38, 5);
pub const SHADE_TIME_AT: At = (130, 4);
pub const SHADE_POS_AT: At = (226, 4);

// ---- volume.bmp / balance.bmp --------------------------------------------
pub const VOLUME: R = r(107, 57, 68, 13);
pub const BALANCE: R = r(177, 57, 38, 13);
pub const SLIDER_THUMB: R = r(15, 422, 14, 11);
pub const SLIDER_THUMB_DOWN: R = r(0, 422, 14, 11);
pub const VOLUME_TRAVEL: i32 = 68 - 14;
pub const BALANCE_TRAVEL: i32 = 38 - 14;

/// Background strip `frame` (0..=27) of the volume bar.
pub fn volume_frame(frame: i32) -> R {
    r(0, frame * 15, 68, 13)
}

pub fn balance_frame(frame: i32) -> R {
    r(9, frame * 15, 38, 13)
}

// ---- posbar.bmp ----------------------------------------------------------
pub const POSBAR: R = r(16, 72, 248, 10);
pub const POS_BG: R = r(0, 0, 248, 10);
pub const POS_THUMB: R = r(248, 0, 29, 10);
pub const POS_THUMB_DOWN: R = r(278, 0, 29, 10);
pub const POS_TRAVEL: i32 = 248 - 29;

// ---- shufrep.bmp ---------------------------------------------------------
/// Toggles have four looks: off, off+pressed, on, on+pressed.
#[derive(Clone, Copy, Debug)]
pub struct Toggle {
    pub off: R,
    pub off_down: R,
    pub on: R,
    pub on_down: R,
    pub at: At,
}

impl Toggle {
    pub fn hit(&self) -> R {
        r(self.at.0, self.at.1, self.off.w, self.off.h)
    }

    pub fn sprite(&self, on: bool, down: bool) -> R {
        match (on, down) {
            (false, false) => self.off,
            (false, true) => self.off_down,
            (true, false) => self.on,
            (true, true) => self.on_down,
        }
    }
}

pub const SHUFFLE: Toggle = Toggle {
    off: r(28, 0, 47, 15),
    off_down: r(28, 15, 47, 15),
    on: r(28, 30, 47, 15),
    on_down: r(28, 45, 47, 15),
    at: (164, 89),
};
pub const REPEAT: Toggle = Toggle {
    off: r(0, 0, 28, 15),
    off_down: r(0, 15, 28, 15),
    on: r(0, 30, 28, 15),
    on_down: r(0, 45, 28, 15),
    at: (210, 89),
};
pub const EQ_TOGGLE: Toggle = Toggle {
    off: r(0, 61, 23, 12),
    off_down: r(46, 61, 23, 12),
    on: r(0, 73, 23, 12),
    on_down: r(46, 73, 23, 12),
    at: (219, 58),
};
pub const PL_TOGGLE: Toggle = Toggle {
    off: r(23, 61, 23, 12),
    off_down: r(69, 61, 23, 12),
    on: r(23, 73, 23, 12),
    on_down: r(69, 73, 23, 12),
    at: (242, 58),
};

// ---- eqmain.bmp ----------------------------------------------------------
pub const EQ_BG: R = r(0, 0, 275, 116);
pub const EQ_TITLE: R = r(0, 149, 275, 14);
pub const EQ_TITLE_ACTIVE: R = r(0, 134, 275, 14);
pub const EQ_THUMB: R = r(0, 164, 11, 11);
pub const EQ_THUMB_DOWN: R = r(0, 176, 11, 11);
pub const EQ_CLOSE: Button = Button {
    up: r(0, 116, 9, 9),
    down: r(0, 125, 9, 9),
    at: (264, 3),
};
pub const EQ_ON: Toggle = Toggle {
    off: r(10, 119, 26, 12),
    off_down: r(128, 119, 26, 12),
    on: r(69, 119, 26, 12),
    on_down: r(187, 119, 26, 12),
    at: (14, 18),
};
pub const EQ_AUTO: Toggle = Toggle {
    off: r(36, 119, 32, 12),
    off_down: r(154, 119, 32, 12),
    on: r(95, 119, 32, 12),
    on_down: r(213, 119, 32, 12),
    at: (40, 18),
};
pub const EQ_PRESETS: Button = Button {
    up: r(224, 164, 44, 12),
    down: r(224, 176, 44, 12),
    at: (217, 18),
};
pub const EQ_GRAPH_BG: R = r(0, 294, 113, 19);
pub const EQ_GRAPH_COLORS: R = r(115, 294, 1, 19);
pub const EQ_PREAMP_LINE: R = r(0, 314, 113, 1);
pub const EQ_GRAPH_AT: At = (86, 17);
pub const EQ_SLIDER_W: i32 = 14;
pub const EQ_SLIDER_H: i32 = 63;
pub const EQ_THUMB_TRAVEL: i32 = 51;
pub const EQ_PREAMP_AT: At = (21, 38);
pub const EQ_BAND_X0: i32 = 78;
pub const EQ_BAND_STEP: i32 = 18;
pub const EQ_SLIDER_Y: i32 = 38;

/// Slider background `frame` (0..=27), packed 14 to a row in eqmain.bmp.
pub fn eq_slider_frame(frame: i32) -> R {
    r(
        13 + (frame % 14) * 15,
        164 + (frame / 14) * 65,
        EQ_SLIDER_W,
        EQ_SLIDER_H,
    )
}

pub fn eq_band_at(band: usize) -> At {
    (EQ_BAND_X0 + band as i32 * EQ_BAND_STEP, EQ_SLIDER_Y)
}

// ---- pledit.bmp ----------------------------------------------------------
pub const PL_MIN_W: i32 = 275;
pub const PL_MIN_H: i32 = 116;
pub const PL_STEP_W: i32 = 25;
pub const PL_STEP_H: i32 = 29;
pub const PL_TOP_H: i32 = 20;
pub const PL_BOTTOM_H: i32 = 38;
pub const PL_LEFT_W: i32 = 12;
pub const PL_RIGHT_W: i32 = 20;

pub const PL_TOP_LEFT: R = r(0, 21, 25, 20);
pub const PL_TOP_LEFT_ACTIVE: R = r(0, 0, 25, 20);
pub const PL_TITLE: R = r(26, 21, 100, 20);
pub const PL_TITLE_ACTIVE: R = r(26, 0, 100, 20);
pub const PL_TOP_TILE: R = r(127, 21, 25, 20);
pub const PL_TOP_TILE_ACTIVE: R = r(127, 0, 25, 20);
pub const PL_TOP_RIGHT: R = r(153, 21, 25, 20);
pub const PL_TOP_RIGHT_ACTIVE: R = r(153, 0, 25, 20);
pub const PL_LEFT_TILE: R = r(0, 42, 12, 29);
pub const PL_RIGHT_TILE: R = r(31, 42, 20, 29);
pub const PL_BOTTOM_LEFT: R = r(0, 72, 125, 38);
pub const PL_BOTTOM_RIGHT: R = r(126, 72, 150, 38);
pub const PL_BOTTOM_TILE: R = r(179, 0, 25, 38);
pub const PL_SCROLL: R = r(52, 53, 8, 18);
pub const PL_SCROLL_ACTIVE: R = r(61, 53, 8, 18);
pub const PL_CLOSE_DOWN: R = r(52, 42, 9, 9);

/// A playlist pop-up menu: the bar drawn beside it, and its items from top
/// to bottom (the bottom one sits on the button), as (item, item pressed).
pub struct PlMenu {
    pub bar: R,
    pub items: &'static [(R, R)],
    /// x of the menu's button inside the bottom-left corner (or from the
    /// right edge, for LIST).
    pub x: i32,
}

pub const PL_ADD: PlMenu = PlMenu {
    bar: r(48, 111, 3, 54),
    items: &[
        (r(0, 111, 22, 18), r(23, 111, 22, 18)), // URL
        (r(0, 130, 22, 18), r(23, 130, 22, 18)), // DIR
        (r(0, 149, 22, 18), r(23, 149, 22, 18)), // FILE
    ],
    x: 14,
};
pub const PL_REM: PlMenu = PlMenu {
    bar: r(100, 111, 3, 72),
    items: &[
        (r(54, 168, 22, 18), r(77, 168, 22, 18)), // MISC
        (r(54, 111, 22, 18), r(77, 111, 22, 18)), // ALL
        (r(54, 130, 22, 18), r(77, 130, 22, 18)), // CROP
        (r(54, 149, 22, 18), r(77, 149, 22, 18)), // SEL
    ],
    x: 43,
};
pub const PL_SEL: PlMenu = PlMenu {
    bar: r(150, 111, 3, 54),
    items: &[
        (r(104, 111, 22, 18), r(127, 111, 22, 18)), // INV
        (r(104, 130, 22, 18), r(127, 130, 22, 18)), // ZERO
        (r(104, 149, 22, 18), r(127, 149, 22, 18)), // ALL
    ],
    x: 72,
};
pub const PL_MISC: PlMenu = PlMenu {
    bar: r(200, 111, 3, 54),
    items: &[
        (r(154, 111, 22, 18), r(177, 111, 22, 18)), // SORT LIST
        (r(154, 130, 22, 18), r(177, 130, 22, 18)), // FILE INFO
        (r(154, 149, 22, 18), r(177, 149, 22, 18)), // MISC OPTIONS
    ],
    x: 101,
};
/// The six mini transport buttons in the bottom-right corner piece (prev,
/// play, pause, stop, next, eject), 10×10 each, relative to its left edge.
pub const PL_MINI_TRANSPORT_AT: (i32, i32) = (3, 22);
pub const PL_LIST: PlMenu = PlMenu {
    bar: r(250, 111, 3, 54),
    items: &[
        (r(204, 111, 22, 18), r(227, 111, 22, 18)), // NEW
        (r(204, 130, 22, 18), r(227, 130, 22, 18)), // SAVE
        (r(204, 149, 22, 18), r(227, 149, 22, 18)), // LOAD
    ],
    x: 44, // from the right edge
};

// ---- text.bmp ------------------------------------------------------------
pub const CHAR_W: i32 = 5;
pub const CHAR_H: i32 = 6;

/// The cell of `c` in text.bmp as (row, column), or `None` for a blank.
pub fn glyph(c: char) -> Option<(i32, i32)> {
    let c = c.to_ascii_lowercase();
    let cell = match c {
        'a'..='z' => (0, c as i32 - 'a' as i32),
        '"' => (0, 26),
        '@' => (0, 27),
        ' ' => (0, 30),
        '0'..='9' => (1, c as i32 - '0' as i32),
        '…' => (1, 10),
        '.' => (1, 11),
        ':' => (1, 12),
        '(' => (1, 13),
        ')' => (1, 14),
        '-' => (1, 15),
        '\'' | '`' => (1, 16),
        '!' => (1, 17),
        '_' => (1, 18),
        '+' => (1, 19),
        '\\' => (1, 20),
        '/' => (1, 21),
        '[' | '<' | '{' => (1, 22),
        ']' | '>' | '}' => (1, 23),
        '^' => (1, 24),
        '&' => (1, 25),
        '%' => (1, 26),
        ',' => (1, 27),
        '=' => (1, 28),
        '$' => (1, 29),
        '#' => (1, 30),
        'å' => (2, 0),
        'ö' => (2, 1),
        'ä' => (2, 2),
        '?' => (2, 3),
        '*' => (2, 4),
        _ => return None,
    };
    Some(cell)
}

pub fn glyph_rect(c: char) -> R {
    let (row, col) = glyph(c).unwrap_or((0, 30));
    r(col * CHAR_W, row * CHAR_H, CHAR_W, CHAR_H)
}
