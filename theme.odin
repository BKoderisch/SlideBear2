package main

import "core:encoding/json"
import "core:fmt"
import "core:os"
import sdl "vendor:sdl2"

THEME_FILE :: "theme.json"

Theme :: struct {
	bg_r:      u8,
	bg_g:      u8,
	bg_b:      u8,
	font_path: string,
	font_size: int,
}

Theme_JSON :: struct {
	bg_r:      u8     `json:"bg_r"`,
	bg_g:      u8     `json:"bg_g"`,
	bg_b:      u8     `json:"bg_b"`,
	font_path: string `json:"font_path"`,
	font_size: int    `json:"font_size"`,
}

default_theme :: proc() -> Theme {
	return Theme{
		bg_r      = 0,
		bg_g      = 0,
		bg_b      = 0,
		font_path = FONT_PATH,
		font_size = FONT_SIZE,
	}
}

load_theme :: proc(path: string) -> Theme {
	data, err0 := os.read_entire_file_from_path(path, context.allocator)
	if err0 != nil {
		fmt.println("theme.json not found, using defaults")
		return default_theme()
	}
	defer delete(data)

	tj: Theme_JSON
	err := json.unmarshal(data, &tj)
	if err != nil {
		fmt.eprintln("theme.json parse error:", err, "- using defaults")
		return default_theme()
	}

	t := Theme{
		bg_r = tj.bg_r,
		bg_g = tj.bg_g,
		bg_b = tj.bg_b,
	}
	if tj.font_path != "" { t.font_path = tj.font_path } else { t.font_path = FONT_PATH }
	if tj.font_size > 0   { t.font_size = tj.font_size } else { t.font_size = FONT_SIZE }
	return t
}

theme_bg_color :: proc(t: Theme) -> sdl.Color {
	return sdl.Color{t.bg_r, t.bg_g, t.bg_b, 255}
}
