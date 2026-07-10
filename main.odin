package main

import "core:c"
import "core:fmt"
import "core:os"
import "core:strings"
import sdl "vendor:sdl2"
import ttf "vendor:sdl2/ttf"

OPERATOR_W  :: 1280
OPERATOR_H  :: 800
BEAMER_W    :: 1280
BEAMER_H    :: 720
FONT_PATH   :: "/System/Library/Fonts/Supplemental/Arial.ttf"
FONT_SIZE   :: 72

PANEL_W        :: i32(320)
ROW_H          :: i32(36)
HEADER_H       :: i32(30)
ADD_BTN_H      :: i32(38)
SETLIST_AREA_H :: i32(252)
REMOVE_W       :: i32(24)
UI_FONT_SZ     :: 22

TILE_W      :: i32(232)
TILE_H      :: i32(132)
TILE_GAP    :: i32(16)
TILE_MARGIN :: i32(20)

PREVIEW_H       :: i32(140)
PREVIEW_LABEL_H :: i32(24)
PREVIEW_BOX_Y   :: i32(TILE_MARGIN + PREVIEW_LABEL_H)
STRIP_H         :: i32(PREVIEW_BOX_Y + PREVIEW_H + 16)

Panel_Click :: enum {
	None,
	Setlist_Select,
	Setlist_Remove,
	Setlist_New,
	Song_Select,
	Song_Remove,
	Add_Open,
	Library_Open,
	Tile,
}

Editor_Field :: enum { Title, Body }

Editor_Click :: enum { None, Field_Title, Field_Body, Save, Cancel }

Shortcut :: struct {
	key:  string,
	desc: string,
}

SHORTCUTS := [?]Shortcut{
	{"ESC",              "Beenden / Dialog schließen"},
	{"B",                "Beamer ein-/ausblenden"},
	{"→ / ↓ / SPACE",    "Nächste Folie"},
	{"← / ↑",            "Vorherige Folie"},
	{"[  ]",             "PPTX-Folie zurück / vor"},
	{"L",                "Zurück zu Liedtext"},
	{"S",                "Ganzes Lied anzeigen (mehrspaltig)"},
	{"N",                "Neuen Song erstellen"},
	{"⌘V / ⌘C / ⌘X",     "Einfügen/Kopieren/Ausschneiden im Song-Editor"},
	{"⌘A",               "Alles markieren im Song-Editor"},
	{"Shift+Klick/Pfeil", "Markieren im Song-Editor"},
	{"⌥+Pfeil / ⌘+Pfeil", "Wort- bzw. Zeilensprung im Song-Editor"},
	{"BACKSPACE",        "Song aus Setlist entfernen"},
	{"H",                "Diese Hilfe ein-/ausblenden"},
}

Beamer_Content :: union {
	^sdl.Surface,
	string,
}

// Clips subsequent draws to [x0, x1) x [y0, y0+h) so a long title can't
// visually run into a fixed-position icon/button to its right. Callers must
// pair this with clip_reset once they're done drawing the clipped text.
clip_until :: proc(renderer: ^sdl.Renderer, x0, y0, x1, h: i32) {
	sdl.RenderSetClipRect(renderer, &sdl.Rect{x = x0, y = y0, w = max(x1 - x0, 0), h = h})
}

clip_reset :: proc(renderer: ^sdl.Renderer) {
	sdl.RenderSetClipRect(renderer, nil)
}

render_text_line :: proc(renderer: ^sdl.Renderer, font: ^ttf.Font, text: string, x, y: i32, color: sdl.Color, scale: f32, size_factor: f32 = 1.0) {
	if len(text) == 0 do return
	cs := strings.clone_to_cstring(text, context.temp_allocator)
	surface := ttf.RenderUTF8_Blended(font, cs, color)
	if surface == nil do return
	defer sdl.FreeSurface(surface)
	texture := sdl.CreateTextureFromSurface(renderer, surface)
	if texture == nil do return
	defer sdl.DestroyTexture(texture)
	tw, th: i32
	sdl.QueryTexture(texture, nil, nil, &tw, &th)
	// texture is rendered at scale*UI_FONT_SZ px; dst is divided by scale so
	// RenderSetScale's upscale lands back on the texture's native resolution.
	// size_factor additionally shrinks the destination (shrink-to-fit tiles).
	dst := sdl.Rect{x = x, y = y, w = i32(f32(tw) / scale * size_factor), h = i32(f32(th) / scale * size_factor)}
	sdl.RenderCopy(renderer, texture, nil, &dst)
}

// Point-space width of `text` if rendered with render_text_line, so a label
// can be right-aligned instead of guessing a fixed x offset that breaks
// whenever the label text or font metrics change.
text_width :: proc(font: ^ttf.Font, text: string, scale: f32) -> i32 {
	if len(text) == 0 do return 0
	cs := strings.clone_to_cstring(text, context.temp_allocator)
	w: c.int
	ttf.SizeUTF8(font, cs, &w, nil)
	return i32(f32(w) / scale)
}

compute_scale :: proc(renderer: ^sdl.Renderer, win: ^sdl.Window) -> f32 {
	draw_w, draw_h: i32
	sdl.GetRendererOutputSize(renderer, &draw_w, &draw_h)
	win_w, win_h: i32
	sdl.GetWindowSize(win, &win_w, &win_h)
	if win_w == 0 do return 1.0
	return f32(draw_w) / f32(win_w)
}

song_matches_search :: proc(title: string, query: string) -> bool {
	if len(query) == 0 do return true
	return strings.contains(
		strings.to_lower(title, context.temp_allocator),
		strings.to_lower(query, context.temp_allocator),
	)
}

tile_cols :: proc(op_w: i32) -> i32 {
	avail := op_w - PANEL_W - 2*TILE_MARGIN
	c := (avail + TILE_GAP) / (TILE_W + TILE_GAP)
	return max(c, 1)
}

// is_live: this tile is what's actually showing on the beamer right now.
// is_cursor: this is the browsing cursor within the currently displayed
// song's tiles - shown as an outline only when it isn't also the live tile,
// so switching songs never looks like it went live until it actually does.
render_tile :: proc(renderer: ^sdl.Renderer, font: ^ttf.Font, sec: Section, label: string, tx, ty: i32, is_live: bool, is_cursor: bool, scale: f32) {
	if is_live {
		sdl.SetRenderDrawColor(renderer, 60, 100, 180, 255)
	} else {
		sdl.SetRenderDrawColor(renderer, 44, 44, 50, 255)
	}
	sdl.RenderFillRect(renderer, &sdl.Rect{x = tx, y = ty, w = TILE_W, h = TILE_H})
	border_col: sdl.Color = {140, 180, 240, 255} if (is_cursor && !is_live) else {80, 80, 90, 255}
	sdl.SetRenderDrawColor(renderer, border_col.r, border_col.g, border_col.b, 255)
	sdl.RenderDrawRect(renderer, &sdl.Rect{x = tx, y = ty, w = TILE_W, h = TILE_H})
	if is_cursor && !is_live {
		sdl.RenderDrawRect(renderer, &sdl.Rect{x = tx + 1, y = ty + 1, w = TILE_W - 2, h = TILE_H - 2})
	}

	// Left accent stripe colored by section kind (verse/chorus/bridge/...) so
	// the type is recognizable at a glance without reading the label text.
	kind_col := section_kind_color(sec.kind)
	sdl.SetRenderDrawColor(renderer, kind_col.r, kind_col.g, kind_col.b, 255)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = tx + 1, y = ty + 1, w = 5, h = TILE_H - 2})

	clip := sdl.Rect{x = tx + 10, y = ty + 1, w = TILE_W - 11, h = TILE_H - 2}
	sdl.RenderSetClipRect(renderer, &clip)

	// Shrink-to-fit instead of wrapping: measure the widest line and the
	// total block height at normal size, then scale everything down
	// uniformly so the whole slide's text stays on one line each and fully
	// visible, rather than wrapping and potentially clipping at the bottom.
	avail_w := f32(TILE_W - 26)
	avail_h := f32(TILE_H - 14)
	base_line_h := f32(24)

	widest := f32(text_width(font, label, scale))
	for line in sec.lines {
		w := f32(text_width(font, line, scale))
		if w > widest { widest = w }
	}
	if widest < 1 { widest = 1 }
	natural_h := base_line_h * f32(1 + len(sec.lines))

	factor := min(min(avail_w / widest, avail_h / natural_h), 1.0)
	factor = max(factor, 0.3)
	line_h := i32(base_line_h * factor)

	render_text_line(renderer, font, label, tx + 16, ty + 7, kind_col, scale, factor)

	text_col: sdl.Color = {255, 255, 255, 255} if is_live else {225, 225, 228, 255}
	ly := ty + 7 + line_h + 4
	for line in sec.lines {
		render_text_line(renderer, font, line, tx + 16, ly, text_col, scale, factor)
		ly += line_h
	}

	sdl.RenderSetClipRect(renderer, nil)
}

render_operator :: proc(
	renderer:        ^sdl.Renderer,
	ui_font:         ^ttf.Font,
	all_songs:       []Song,
	song_idx:        int,
	section_idx:     int,
	live_song_idx:   int,
	live_section_idx: int,
	playlists:       []Playlist,
	active_playlist: int,
	in_slide_mode:   bool,
	naming:          bool,
	name_buf:        string,
	preview_w:       i32,
	op_w, op_h:      i32,
	scale:           f32,
) {
	sdl.SetRenderDrawColor(renderer, 28, 28, 30, 255)
	sdl.RenderClear(renderer)
	sdl.RenderSetScale(renderer, scale, scale)

	sdl.SetRenderDrawColor(renderer, 20, 20, 22, 255)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = 0, y = 0, w = PANEL_W, h = op_h})

	// ---- Setlists ----
	if naming {
		sdl.SetRenderDrawColor(renderer, 55, 55, 60, 255)
		sdl.RenderFillRect(renderer, &sdl.Rect{x = 0, y = 0, w = PANEL_W, h = HEADER_H})
		display := strings.concatenate({"Name: ", name_buf, "|"}, context.temp_allocator)
		render_text_line(renderer, ui_font, display, 8, 5, {220, 220, 220, 255}, scale)
	} else {
		render_text_line(renderer, ui_font, "SETLISTS", 8, 5, {150, 150, 160, 255}, scale)
		new_label := "+ Neu"
		render_text_line(renderer, ui_font, new_label, PANEL_W - 8 - text_width(ui_font, new_label, scale), 5, {130, 200, 130, 255}, scale)
	}

	for pl, i in playlists {
		item_y := HEADER_H + i32(i)*ROW_H
		if item_y + ROW_H > SETLIST_AREA_H do break
		if i == active_playlist {
			sdl.SetRenderDrawColor(renderer, 60, 100, 180, 255)
			sdl.RenderFillRect(renderer, &sdl.Rect{x = 0, y = item_y, w = PANEL_W, h = ROW_H})
		} else if i % 2 == 0 {
			sdl.SetRenderDrawColor(renderer, 33, 33, 36, 255)
			sdl.RenderFillRect(renderer, &sdl.Rect{x = 0, y = item_y, w = PANEL_W, h = ROW_H})
		}
		col: sdl.Color = {240, 240, 240, 255} if i == active_playlist else {185, 185, 190, 255}
		clip_until(renderer, 10, item_y, PANEL_W - REMOVE_W - 4, ROW_H)
		render_text_line(renderer, ui_font, pl.name, 10, item_y + 8, col, scale)
		clip_reset(renderer)
		if len(playlists) > 1 {
			render_text_line(renderer, ui_font, "×", PANEL_W - 20, item_y + 8, {200, 90, 90, 255}, scale)
		}
	}

	sdl.SetRenderDrawColor(renderer, 60, 60, 66, 255)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = 0, y = SETLIST_AREA_H, w = PANEL_W, h = 2})

	// ---- Songs in active setlist ----
	sy := SETLIST_AREA_H + 4
	render_text_line(renderer, ui_font, "SONGS", 8, sy + 4, {150, 150, 160, 255}, scale)
	lib_label := "Bibliothek"
	lib_label_x := PANEL_W - 8 - text_width(ui_font, lib_label, scale)
	render_text_line(renderer, ui_font, lib_label, lib_label_x, sy + 4, {150, 170, 210, 255}, scale)

	add_y := sy + HEADER_H
	sdl.SetRenderDrawColor(renderer, 40, 70, 45, 255)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = 8, y = add_y, w = PANEL_W - 16, h = ADD_BTN_H})
	sdl.SetRenderDrawColor(renderer, 90, 150, 95, 255)
	sdl.RenderDrawRect(renderer, &sdl.Rect{x = 8, y = add_y, w = PANEL_W - 16, h = ADD_BTN_H})
	render_text_line(renderer, ui_font, "+  Song hinzufügen", 20, add_y + 8, {200, 240, 200, 255}, scale)

	active := playlists[active_playlist].songs[:]
	ly := add_y + ADD_BTN_H + 6
	if len(active) == 0 {
		render_text_line(renderer, ui_font, "Noch keine Songs.", 12, ly + 4, {90, 90, 95, 255}, scale)
	}
	for lib_idx, pos in active {
		item_y := ly + i32(pos)*ROW_H
		if item_y + ROW_H > op_h - 4 do break
		if lib_idx == song_idx {
			sdl.SetRenderDrawColor(renderer, 60, 100, 180, 255)
			sdl.RenderFillRect(renderer, &sdl.Rect{x = 0, y = item_y, w = PANEL_W, h = ROW_H})
		} else if pos % 2 == 0 {
			sdl.SetRenderDrawColor(renderer, 33, 33, 36, 255)
			sdl.RenderFillRect(renderer, &sdl.Rect{x = 0, y = item_y, w = PANEL_W, h = ROW_H})
		}
		col: sdl.Color = {240, 240, 240, 255} if lib_idx == song_idx else {185, 185, 190, 255}
		label := fmt.tprintf("%d. %s", pos + 1, all_songs[lib_idx].title)
		clip_until(renderer, 10, item_y, PANEL_W - REMOVE_W - 4, ROW_H)
		render_text_line(renderer, ui_font, label, 10, item_y + 8, col, scale)
		clip_reset(renderer)
		render_text_line(renderer, ui_font, "×", PANEL_W - 20, item_y + 8, {200, 90, 90, 255}, scale)
	}

	sdl.SetRenderDrawColor(renderer, 50, 50, 56, 255)
	sdl.RenderDrawLine(renderer, PANEL_W, 0, PANEL_W, op_h)

	// ---- Slide tiles ----
	if len(active) > 0 {
		song := all_songs[song_idx]
		title_max_x := op_w - preview_w - TILE_MARGIN - 12
		clip_until(renderer, PANEL_W + TILE_MARGIN, 0, title_max_x, PREVIEW_BOX_Y)
		render_text_line(renderer, ui_font, song.title, PANEL_W + TILE_MARGIN, TILE_MARGIN, {200, 200, 205, 255}, scale)
		clip_reset(renderer)
		cols := tile_cols(op_w)
		start_x := PANEL_W + TILE_MARGIN
		start_y := STRIP_H
		labels := song_section_display_labels(song.sections)
		for sec, i in song.sections {
			col := i32(i) % cols
			row := i32(i) / cols
			tx := start_x + col*(TILE_W + TILE_GAP)
			ty := start_y + row*(TILE_H + TILE_GAP)
			if ty + TILE_H > op_h do break
			is_live := !in_slide_mode && song_idx == live_song_idx && i == live_section_idx
			is_cursor := !in_slide_mode && i == section_idx
			render_tile(renderer, ui_font, sec, labels[i], tx, ty, is_live, is_cursor, scale)
		}
	} else {
		render_text_line(renderer, ui_font, "Setlist ist leer.", PANEL_W + TILE_MARGIN, STRIP_H, {90, 90, 95, 255}, scale)
		render_text_line(renderer, ui_font, "Songs über \"+ Song hinzufügen\" wählen.", PANEL_W + TILE_MARGIN, STRIP_H + 30, {90, 90, 95, 255}, scale)
	}
}

add_box :: proc(op_w, op_h: i32) -> (bx, by, bw, bh: i32) {
	bw = min(640, op_w - 40)
	bh = min(600, op_h - 40)
	bx = (op_w - bw) / 2
	by = (op_h - bh) / 2
	return
}

// This dialog only ever adds/removes songs to/from the active playlist -
// editing and deleting songs themselves lives in the separate Library
// overlay (render_library_overlay / library_overlay_click).
Add_Click :: enum { None, Close, Toggle }

render_add_overlay :: proc(
	renderer:    ^sdl.Renderer,
	ui_font:     ^ttf.Font,
	all_songs:   []Song,
	active:      []int,
	search_q:    string,
	op_w, op_h:  i32,
	scale:       f32,
) {
	sdl.SetRenderDrawBlendMode(renderer, .BLEND)
	sdl.SetRenderDrawColor(renderer, 0, 0, 0, 200)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = 0, y = 0, w = op_w, h = op_h})

	bx, by, bw, bh := add_box(op_w, op_h)
	sdl.SetRenderDrawColor(renderer, 32, 32, 36, 255)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = bx, y = by, w = bw, h = bh})
	sdl.SetRenderDrawColor(renderer, 90, 90, 100, 255)
	sdl.RenderDrawRect(renderer, &sdl.Rect{x = bx, y = by, w = bw, h = bh})

	render_text_line(renderer, ui_font, "Song hinzufügen", bx + 16, by + 14, {225, 225, 230, 255}, scale)
	render_text_line(renderer, ui_font, "×", bx + bw - 28, by + 10, {200, 120, 120, 255}, scale)

	// Search field
	sx := bx + 16
	sw := bw - 32
	sfy := by + 52
	sdl.SetRenderDrawColor(renderer, 55, 55, 60, 255)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = sx, y = sfy, w = sw, h = 34})
	clip_until(renderer, sx, sfy, sx + sw, 34)
	if len(search_q) > 0 {
		display := strings.concatenate({search_q, "|"}, context.temp_allocator)
		render_text_line(renderer, ui_font, display, sx + 8, sfy + 7, {225, 225, 225, 255}, scale)
	} else {
		render_text_line(renderer, ui_font, "Suche...", sx + 8, sfy + 7, {120, 120, 125, 255}, scale)
	}
	clip_reset(renderer)

	list_top := by + 100
	row := 0
	for song, i in all_songs {
		if song.deleted do continue
		if !song_matches_search(song.title, search_q) do continue
		item_y := list_top + i32(row)*ROW_H
		if item_y + ROW_H > by + bh - 12 do break
		already := false
		for a in active { if a == i { already = true; break } }
		if row % 2 == 0 {
			sdl.SetRenderDrawColor(renderer, 40, 40, 44, 255)
			sdl.RenderFillRect(renderer, &sdl.Rect{x = bx + 8, y = item_y, w = bw - 16, h = ROW_H})
		}
		col: sdl.Color = {110, 130, 110, 255} if already else {225, 225, 228, 255}
		clip_until(renderer, bx + 18, item_y, bx + bw - 140, ROW_H)
		render_text_line(renderer, ui_font, song.title, bx + 18, item_y + 8, col, scale)
		clip_reset(renderer)

		add_col: sdl.Color = {130, 200, 130, 255}
		add_sym := "✓ entfernen" if already else "+ hinzufügen"
		render_text_line(renderer, ui_font, add_sym, bx + bw - 130, item_y + 8, add_col, scale)
		row += 1
	}

	sdl.SetRenderDrawBlendMode(renderer, .NONE)
}

add_overlay_click :: proc(cx, cy: i32, all_songs: []Song, search_q: string, op_w, op_h: i32) -> (click: Add_Click, idx: int) {
	bx, by, bw, bh := add_box(op_w, op_h)
	if cx < bx || cx >= bx + bw || cy < by || cy >= by + bh do return .Close, 0
	if cy < by + 40 && cx >= bx + bw - 40 do return .Close, 0

	list_top := by + 100
	if cy < list_top do return .None, 0
	target := int((cy - list_top) / ROW_H)
	row := 0
	for song, i in all_songs {
		if song.deleted do continue
		if !song_matches_search(song.title, search_q) do continue
		if row == target { return .Toggle, i }
		row += 1
	}
	return .None, 0
}

// ── Library overlay (manage all songs: create, edit, permanently delete) ───

Library_Click :: enum { None, Close, New_Song, Edit, Delete }

render_library_overlay :: proc(
	renderer:           ^sdl.Renderer,
	ui_font:            ^ttf.Font,
	all_songs:          []Song,
	search_q:           string,
	delete_pending_idx: int,
	op_w, op_h:         i32,
	scale:              f32,
) {
	sdl.SetRenderDrawBlendMode(renderer, .BLEND)
	sdl.SetRenderDrawColor(renderer, 0, 0, 0, 200)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = 0, y = 0, w = op_w, h = op_h})

	bx, by, bw, bh := add_box(op_w, op_h)
	sdl.SetRenderDrawColor(renderer, 32, 32, 36, 255)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = bx, y = by, w = bw, h = bh})
	sdl.SetRenderDrawColor(renderer, 90, 90, 100, 255)
	sdl.RenderDrawRect(renderer, &sdl.Rect{x = bx, y = by, w = bw, h = bh})

	render_text_line(renderer, ui_font, "Bibliothek", bx + 16, by + 14, {225, 225, 230, 255}, scale)
	render_text_line(renderer, ui_font, "+ Neuer Song", bx + bw - 190, by + 14, {130, 200, 130, 255}, scale)
	render_text_line(renderer, ui_font, "×", bx + bw - 28, by + 10, {200, 120, 120, 255}, scale)

	sx := bx + 16
	sw := bw - 32
	sfy := by + 52
	sdl.SetRenderDrawColor(renderer, 55, 55, 60, 255)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = sx, y = sfy, w = sw, h = 34})
	clip_until(renderer, sx, sfy, sx + sw, 34)
	if len(search_q) > 0 {
		display := strings.concatenate({search_q, "|"}, context.temp_allocator)
		render_text_line(renderer, ui_font, display, sx + 8, sfy + 7, {225, 225, 225, 255}, scale)
	} else {
		render_text_line(renderer, ui_font, "Suche...", sx + 8, sfy + 7, {120, 120, 125, 255}, scale)
	}
	clip_reset(renderer)

	list_top := by + 100
	row := 0
	for song, i in all_songs {
		if song.deleted do continue
		if !song_matches_search(song.title, search_q) do continue
		item_y := list_top + i32(row)*ROW_H
		if item_y + ROW_H > by + bh - 12 do break
		if i == delete_pending_idx {
			sdl.SetRenderDrawColor(renderer, 70, 35, 35, 255)
			sdl.RenderFillRect(renderer, &sdl.Rect{x = bx + 8, y = item_y, w = bw - 16, h = ROW_H})
		} else if row % 2 == 0 {
			sdl.SetRenderDrawColor(renderer, 40, 40, 44, 255)
			sdl.RenderFillRect(renderer, &sdl.Rect{x = bx + 8, y = item_y, w = bw - 16, h = ROW_H})
		}
		clip_until(renderer, bx + 18, item_y, bx + bw - 120, ROW_H)
		render_text_line(renderer, ui_font, song.title, bx + 18, item_y + 8, {225, 225, 228, 255}, scale)
		clip_reset(renderer)
		render_text_line(renderer, ui_font, "✎", bx + bw - 60, item_y + 8, {150, 170, 210, 255}, scale)
		if i == delete_pending_idx {
			render_text_line(renderer, ui_font, "sicher?", bx + bw - 112, item_y + 8, {240, 140, 140, 255}, scale)
		} else {
			render_text_line(renderer, ui_font, "×", bx + bw - 92, item_y + 8, {200, 100, 100, 255}, scale)
		}
		row += 1
	}

	sdl.SetRenderDrawBlendMode(renderer, .NONE)
}

library_overlay_click :: proc(cx, cy: i32, all_songs: []Song, search_q: string, op_w, op_h: i32) -> (click: Library_Click, idx: int) {
	bx, by, bw, bh := add_box(op_w, op_h)
	if cx < bx || cx >= bx + bw || cy < by || cy >= by + bh do return .Close, 0
	if cy < by + 40 {
		if cx >= bx + bw - 40 do return .Close, 0
		if cx >= bx + bw - 190 && cx < bx + bw - 44 do return .New_Song, 0
	}

	list_top := by + 100
	if cy < list_top do return .None, 0
	target := int((cy - list_top) / ROW_H)
	row := 0
	for song, i in all_songs {
		if song.deleted do continue
		if !song_matches_search(song.title, search_q) do continue
		if row == target {
			if cx >= bx + bw - 76 { return .Edit, i }
			if cx >= bx + bw - 112 { return .Delete, i }
			return .Edit, i
		}
		row += 1
	}
	return .None, 0
}

// ── Song editor overlay (create a new song) ─────────────────────────────────

editor_box :: proc(op_w, op_h: i32) -> (bx, by, bw, bh: i32) {
	bw = min(700, op_w - 40)
	bh = min(600, op_h - 40)
	bx = (op_w - bw) / 2
	by = (op_h - bh) / 2
	return
}

// Draws the caret (no selection) or a selection highlight rect (has one) for
// a single line of text at (line_x, line_y), given byte offsets clamped to
// that line's own range beforehand by the caller.
draw_caret_or_selection :: proc(renderer: ^sdl.Renderer, font: ^ttf.Font, line: string, line_x, line_y: i32, lo, hi: int, has_sel: bool, scale: f32) {
	if has_sel {
		x1 := text_x_for_offset(font, line, lo, scale)
		x2 := text_x_for_offset(font, line, hi, scale)
		sdl.SetRenderDrawColor(renderer, 70, 110, 180, 160)
		sdl.RenderFillRect(renderer, &sdl.Rect{x = line_x + x1, y = line_y - 2, w = max(x2 - x1, 2), h = 26})
	} else {
		cx := text_x_for_offset(font, line, lo, scale)
		sdl.SetRenderDrawColor(renderer, 230, 230, 230, 255)
		sdl.RenderFillRect(renderer, &sdl.Rect{x = line_x + cx, y = line_y - 2, w = 2, h = 24})
	}
}

render_editor_overlay :: proc(
	renderer:   ^sdl.Renderer,
	ui_font:    ^ttf.Font,
	title_buf:  string,
	body_buf:   string,
	field:      Editor_Field,
	cursor:     int,
	anchor:     int,
	scroll:     ^int,
	edit_idx:   int,
	op_w, op_h: i32,
	scale:      f32,
) {
	sdl.SetRenderDrawBlendMode(renderer, .BLEND)
	sdl.SetRenderDrawColor(renderer, 0, 0, 0, 200)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = 0, y = 0, w = op_w, h = op_h})

	bx, by, bw, bh := editor_box(op_w, op_h)
	sdl.SetRenderDrawColor(renderer, 32, 32, 36, 255)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = bx, y = by, w = bw, h = bh})
	sdl.SetRenderDrawColor(renderer, 90, 90, 100, 255)
	sdl.RenderDrawRect(renderer, &sdl.Rect{x = bx, y = by, w = bw, h = bh})

	header := "Song bearbeiten" if edit_idx >= 0 else "Neuer Song"
	render_text_line(renderer, ui_font, header, bx + 16, by + 14, {225, 225, 230, 255}, scale)
	render_text_line(renderer, ui_font, "×", bx + bw - 28, by + 10, {200, 120, 120, 255}, scale)

	has_sel := cursor != anchor
	sel_lo := min(cursor, anchor)
	sel_hi := max(cursor, anchor)

	// Title field
	tx := bx + 16
	tw := bw - 32
	tfy := by + 50
	title_col: u8 = 62 if field == .Title else 48
	sdl.SetRenderDrawColor(renderer, title_col, title_col, title_col + 4, 255)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = tx, y = tfy, w = tw, h = 34})
	clip_until(renderer, tx, tfy, tx + tw, 34)
	if field == .Title {
		draw_caret_or_selection(renderer, ui_font, title_buf, tx + 8, tfy + 7, clamp(sel_lo, 0, len(title_buf)), clamp(sel_hi, 0, len(title_buf)), has_sel, scale)
	}
	if len(title_buf) > 0 {
		render_text_line(renderer, ui_font, title_buf, tx + 8, tfy + 7, {225, 225, 225, 255}, scale)
	} else if field != .Title {
		render_text_line(renderer, ui_font, "Titel...", tx + 8, tfy + 7, {120, 120, 125, 255}, scale)
	}
	clip_reset(renderer)

	// Format hint
	hint_y := tfy + 44
	render_text_line(renderer, ui_font, "Format: Zeile mit \"Strophe 1\" / \"Vers 1\" / \"Refrain\" / \"Bridge\" / \"Intro\" ... startet eine neue Folie.", tx, hint_y, {130, 130, 138, 255}, scale)
	render_text_line(renderer, ui_font, "Zeile mit nur --- teilt eine lange Strophe in mehrere Folien auf.", tx, hint_y + 20, {130, 130, 138, 255}, scale)

	// Body textarea
	body_y := hint_y + 46
	body_h := by + bh - body_y - 50
	body_col: u8 = 62 if field == .Body else 48
	sdl.SetRenderDrawColor(renderer, body_col, body_col, body_col + 4, 255)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = tx, y = body_y, w = tw, h = body_h})

	if len(body_buf) == 0 && field != .Body {
		render_text_line(renderer, ui_font, "Vers 1", tx + 8, body_y + 8, {120, 120, 125, 255}, scale)
		render_text_line(renderer, ui_font, "Zeile 1", tx + 8, body_y + 32, {120, 120, 125, 255}, scale)
		render_text_line(renderer, ui_font, "Zeile 2", tx + 8, body_y + 56, {120, 120, 125, 255}, scale)
		render_text_line(renderer, ui_font, "", tx + 8, body_y + 80, {120, 120, 125, 255}, scale)
		render_text_line(renderer, ui_font, "Refrain", tx + 8, body_y + 104, {120, 120, 125, 255}, scale)
	} else {
		lines := strings.split(body_buf, "\n", context.temp_allocator)
		visible_lines := max((body_h - 8) / 24, 1)

		// Auto-scroll so the caret's line is always visible.
		if field == .Body {
			cursor_line := 0
			{
				off := 0
				for l, li in lines {
					if cursor <= off + len(l) { cursor_line = li; break }
					off += len(l) + 1
				}
			}
			if cursor_line < scroll^ { scroll^ = cursor_line }
			if cursor_line >= scroll^ + int(visible_lines) { scroll^ = cursor_line - int(visible_lines) + 1 }
		}
		max_scroll := max(len(lines) - int(visible_lines), 0)
		scroll^ = clamp(scroll^, 0, max_scroll)

		clip_until(renderer, tx, body_y, tx + tw, body_h)
		yy := body_y + 8
		byte_off := 0
		for l in lines[:scroll^] { byte_off += len(l) + 1 }
		for line in lines[scroll^:] {
			line_end := byte_off + len(line)
			if yy + 24 > body_y + body_h do break
			if field == .Body {
				lo := clamp(sel_lo - byte_off, 0, len(line))
				hi := clamp(sel_hi - byte_off, 0, len(line))
				line_has_sel := has_sel && sel_lo <= line_end && sel_hi >= byte_off
				show_caret := !has_sel && cursor >= byte_off && cursor <= line_end
				if line_has_sel || show_caret {
					draw_caret_or_selection(renderer, ui_font, line, tx + 8, yy, lo, hi, line_has_sel, scale)
				}
			}
			render_text_line(renderer, ui_font, line, tx + 8, yy, {225, 225, 228, 255}, scale)
			yy += 24
			byte_off = line_end + 1
		}
		clip_reset(renderer)

		if max_scroll > 0 {
			track_h := body_h - 8
			thumb_h := max(track_h * i32(visible_lines) / i32(len(lines)), 16)
			thumb_y := body_y + 4 + i32(scroll^) * (track_h - thumb_h) / i32(max_scroll)
			sdl.SetRenderDrawColor(renderer, 90, 90, 100, 255)
			sdl.RenderFillRect(renderer, &sdl.Rect{x = tx + tw - 5, y = thumb_y, w = 3, h = thumb_h})
		}
	}

	// Buttons
	by_btn := by + bh - 40
	sdl.SetRenderDrawColor(renderer, 40, 70, 45, 255)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = bx + bw - 140, y = by_btn, w = 124, h = 30})
	render_text_line(renderer, ui_font, "Speichern", bx + bw - 128, by_btn + 5, {200, 240, 200, 255}, scale)
	render_text_line(renderer, ui_font, "TAB wechselt Titel/Text, ESC bricht ab", tx, by_btn + 8, {110, 110, 118, 255}, scale)

	sdl.SetRenderDrawBlendMode(renderer, .NONE)
}

// Byte offset within the given field at (cx, cy) - shared by the initial
// click (editor_click) and by mouse-drag selection updates (MOUSEMOTION),
// where the field is already known and clamped rather than classified.
editor_field_offset :: proc(cx, cy: i32, field: Editor_Field, title, body: string, scroll: int, ui_font: ^ttf.Font, scale: f32, op_w, op_h: i32) -> int {
	bx, by, _, _ := editor_box(op_w, op_h)
	tfy := by + 50
	if field == .Title {
		return text_offset_for_x(ui_font, title, cx - (bx + 16 + 8), scale)
	}
	body_y := tfy + 44 + 46
	line_idx := max(int((cy - (body_y + 8)) / 24), 0) + scroll
	lines := strings.split(body, "\n", context.temp_allocator)
	byte_off := 0
	for l, li in lines {
		if li == line_idx {
			col := text_offset_for_x(ui_font, l, cx - (bx + 16 + 8), scale)
			return byte_off + col
		}
		byte_off += len(l) + 1
	}
	return len(body)
}

editor_click :: proc(
	cx, cy: i32,
	title, body: string,
	scroll: int,
	ui_font: ^ttf.Font,
	scale: f32,
	op_w, op_h: i32,
) -> (click: Editor_Click, cursor: int) {
	bx, by, bw, bh := editor_box(op_w, op_h)
	if cx < bx || cx >= bx + bw || cy < by || cy >= by + bh do return .Cancel, 0
	if cy < by + 40 && cx >= bx + bw - 40 do return .Cancel, 0

	tfy := by + 50
	if cy >= tfy && cy < tfy + 34 {
		return .Field_Title, editor_field_offset(cx, cy, .Title, title, body, scroll, ui_font, scale, op_w, op_h)
	}

	body_y := tfy + 44 + 46
	body_h := by + bh - body_y - 50
	if cy >= body_y && cy < body_y + body_h {
		return .Field_Body, editor_field_offset(cx, cy, .Body, title, body, scroll, ui_font, scale, op_w, op_h)
	}

	by_btn := by + bh - 40
	if cy >= by_btn && cy < by_btn + 30 && cx >= bx + bw - 140 && cx < bx + bw - 16 { return .Save, 0 }

	return .None, 0
}

render_help_overlay :: proc(renderer: ^sdl.Renderer, ui_font: ^ttf.Font, op_w, op_h: i32, scale: f32) {
	sdl.SetRenderDrawBlendMode(renderer, .BLEND)
	sdl.SetRenderDrawColor(renderer, 0, 0, 0, 200)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = 0, y = 0, w = op_w, h = op_h})

	box_w := i32(540)
	box_h := i32(40 + len(SHORTCUTS) * 30)
	box_x := (op_w - box_w) / 2
	box_y := (op_h - box_h) / 2
	sdl.SetRenderDrawColor(renderer, 30, 30, 34, 240)
	sdl.RenderFillRect(renderer, &sdl.Rect{x = box_x, y = box_y, w = box_w, h = box_h})
	sdl.SetRenderDrawColor(renderer, 80, 80, 88, 255)
	sdl.RenderDrawRect(renderer, &sdl.Rect{x = box_x, y = box_y, w = box_w, h = box_h})

	render_text_line(renderer, ui_font, "Tastenkürzel", box_x + 16, box_y + 10, {220, 220, 220, 255}, scale)
	yy := box_y + 40
	for s in SHORTCUTS {
		render_text_line(renderer, ui_font, s.key, box_x + 16, yy, {150, 190, 255, 255}, scale)
		render_text_line(renderer, ui_font, s.desc, box_x + 220, yy, {200, 200, 200, 255}, scale)
		yy += 30
	}
	sdl.SetRenderDrawBlendMode(renderer, .NONE)
}

// Mirrors whatever the beamer window currently shows into a small thumbnail
// in the operator window, so the operator doesn't have to look at the
// projector/second window to know what the audience sees. The caller has
// already filled surf with the right content (normal / show-all / blank).
preview_blit :: proc(
	renderer: ^sdl.Renderer,
	surf:     ^sdl.Surface,
	w, h:     i32,
	box_x, box_y: i32,
	blanked:  bool,
	ui_font:  ^ttf.Font,
	scale:    f32,
) {
	tex := sdl.CreateTextureFromSurface(renderer, surf)
	if tex == nil do return
	defer sdl.DestroyTexture(tex)

	render_text_line(renderer, ui_font, "Vorschau", box_x, box_y - PREVIEW_LABEL_H, {110, 110, 118, 255}, scale)
	sdl.SetRenderDrawColor(renderer, 80, 80, 88, 255)
	sdl.RenderDrawRect(renderer, &sdl.Rect{x = box_x - 2, y = box_y - 2, w = w + 4, h = h + 4})
	sdl.RenderCopy(renderer, tex, nil, &sdl.Rect{x = box_x, y = box_y, w = w, h = h})

	if blanked {
		render_text_line(renderer, ui_font, "BLANK", box_x + w/2 - 30, box_y + h/2 - 10, {200, 90, 90, 255}, scale)
	}
}

panel_click :: proc(
	cx, cy:          i32,
	all_songs:       []Song,
	song_idx:        int,
	active:          []int,
	num_playlists:   int,
	sections_len:    int,
	naming:          bool,
	ui_font:         ^ttf.Font,
	scale:           f32,
	op_w, op_h:      i32,
) -> (click: Panel_Click, idx: int) {
	if cx < PANEL_W {
		if cy < SETLIST_AREA_H {
			if cy < HEADER_H {
				if naming do return .None, 0
				new_x := PANEL_W - 8 - text_width(ui_font, "+ Neu", scale)
				if cx >= new_x - 8 { return .Setlist_New, 0 }
				return .None, 0
			}
			pos := int((cy - HEADER_H) / ROW_H)
			if pos < num_playlists {
				if cx >= PANEL_W - REMOVE_W && num_playlists > 1 { return .Setlist_Remove, pos }
				return .Setlist_Select, pos
			}
			return .None, 0
		}

		sy := SETLIST_AREA_H + 4
		lib_x := PANEL_W - 8 - text_width(ui_font, "Bibliothek", scale)
		if cy >= sy && cy < sy + HEADER_H && cx >= lib_x - 8 { return .Library_Open, 0 }
		add_y := sy + HEADER_H
		if cy >= add_y && cy < add_y + ADD_BTN_H { return .Add_Open, 0 }
		ly := add_y + ADD_BTN_H + 6
		if cy >= ly {
			pos := int((cy - ly) / ROW_H)
			if pos < len(active) {
				if cx >= PANEL_W - REMOVE_W { return .Song_Remove, pos }
				return .Song_Select, active[pos]
			}
		}
		return .None, 0
	}

	// Slide tiles
	if len(active) == 0 do return .None, 0
	cols := tile_cols(op_w)
	start_x := PANEL_W + TILE_MARGIN
	start_y := STRIP_H
	rx := cx - start_x
	ry := cy - start_y
	if rx < 0 || ry < 0 do return .None, 0
	col := rx / (TILE_W + TILE_GAP)
	if col >= cols do return .None, 0
	if rx - col*(TILE_W + TILE_GAP) > TILE_W do return .None, 0
	row := ry / (TILE_H + TILE_GAP)
	if ry - row*(TILE_H + TILE_GAP) > TILE_H do return .None, 0
	i := int(row*cols + col)
	if i < sections_len { return .Tile, i }
	return .None, 0
}

setup_beamer_window :: proc(win: ^sdl.Window) -> (w, h: i32) {
	n := sdl.GetNumVideoDisplays()

	if n >= 2 {
		bounds: sdl.Rect
		sdl.GetDisplayBounds(1, &bounds)
		sdl.SetWindowPosition(win, bounds.x, bounds.y)
		sdl.SetWindowSize(win, bounds.w, bounds.h)
		sdl.SetWindowFullscreen(win, sdl.WINDOW_FULLSCREEN_DESKTOP)
	} else {
		sdl.SetWindowSize(win, BEAMER_W / 2, BEAMER_H / 2)
		sdl.SetWindowPosition(win, OPERATOR_W + 20, 100)
		fmt.println("Nur ein Monitor: Beamer als Vorschaufenster")
	}

	// Query the real surface (drawable pixel) size rather than GetWindowSize
	// (point space), since ALLOW_HIGHDPI can make them differ on Retina displays.
	surf := sdl.GetWindowSurface(win)
	fmt.printf("Beamer Fenster: %dx%d px\n", surf.w, surf.h)
	return surf.w, surf.h
}

main :: proc() {
	if sdl.Init(sdl.INIT_VIDEO) != 0 {
		fmt.eprintln("SDL Init failed:", sdl.GetError())
		return
	}
	defer sdl.Quit()

	if ttf.Init() != 0 {
		fmt.eprintln("TTF Init failed:", ttf.GetError())
		return
	}
	defer ttf.Quit()

	theme := load_theme(THEME_FILE)

	ui_font := ttf.OpenFont(FONT_PATH, UI_FONT_SZ)
	if ui_font == nil { fmt.eprintln("OpenFont (ui) failed:", ttf.GetError()); return }
	defer ttf.CloseFont(ui_font)

	op_win := sdl.CreateWindow("Slide Bear 2 - Operator", 100, 100, OPERATOR_W, OPERATOR_H, {.SHOWN, .RESIZABLE, .ALLOW_HIGHDPI})
	if op_win == nil { fmt.eprintln(sdl.GetError()); return }
	defer sdl.DestroyWindow(op_win)

	beamer_win := sdl.CreateWindow("Slide Bear 2 - Beamer", 0, 0, BEAMER_W, BEAMER_H, {.SHOWN, .ALLOW_HIGHDPI})
	if beamer_win == nil { fmt.eprintln(sdl.GetError()); return }
	defer sdl.DestroyWindow(beamer_win)

	op_renderer := sdl.CreateRenderer(op_win, -1, {.ACCELERATED, .PRESENTVSYNC})
	if op_renderer == nil { fmt.eprintln(sdl.GetError()); return }
	defer sdl.DestroyRenderer(op_renderer)

	op_id := sdl.GetWindowID(op_win)

	bm_w, bm_h := setup_beamer_window(beamer_win)
	bm := Beamer{window = beamer_win, w = bm_w, h = bm_h}

	// theme.font_size is tuned for a bm.h == BEAMER_H beamer (the normal
	// full-size projector case). Scale it proportionally so the single-monitor
	// fallback preview window (a different, smaller real pixel size) shows
	// text at the same relative size instead of overflowing or looking tiny.
	beamer_font_size := max(8, i32(f32(theme.font_size) * f32(bm_h) / f32(BEAMER_H)))
	beamer_font := ttf.OpenFont(
		strings.clone_to_cstring(theme.font_path, context.temp_allocator),
		beamer_font_size,
	)
	if beamer_font == nil { fmt.eprintln("OpenFont (beamer) failed:", ttf.GetError()); return }
	defer ttf.CloseFont(beamer_font)

	// Offscreen thumbnail that mirrors the beamer surface into the operator
	// window. Rendered supersampled (PREVIEW_SS x) and downscaled on blit,
	// same anti-aliasing trick as the HiDPI operator text - a tiny surface
	// rendered 1:1 makes small/shrunk text look noticeably blurry.
	PREVIEW_SS :: 2
	preview_w := i32(f32(PREVIEW_H) * f32(bm_w) / f32(bm_h))
	preview_surf_w := preview_w * PREVIEW_SS
	preview_surf_h := PREVIEW_H * PREVIEW_SS
	preview_surf := sdl.CreateRGBSurfaceWithFormat(0, preview_surf_w, preview_surf_h, 32, u32(sdl.PixelFormatEnum.RGBA32))
	defer sdl.FreeSurface(preview_surf)
	preview_font_size := max(8, i32(f32(theme.font_size) * f32(PREVIEW_H) / f32(bm_h)) * PREVIEW_SS)
	preview_font := ttf.OpenFont(
		strings.clone_to_cstring(theme.font_path, context.temp_allocator),
		preview_font_size,
	)
	if preview_font == nil { fmt.eprintln("OpenFont (preview) failed:", ttf.GetError()); return }
	defer ttf.CloseFont(preview_font)

	slides: []^sdl.Surface
	defer {
		for s in slides { if s != nil { sdl.FreeSurface(s) } }
		delete(slides)
	}
	if len(os.args) >= 2 {
		fmt.println("Lade PPTX:", os.args[1])
		slides = import_pptx(os.args[1])
	}

	library := load_song_library("songs")
	defer delete(library) // ownership of each Song moves into all_songs below

	demo_src  := "[Verse 1]\nAmazing grace how sweet the sound\nThat saved a wretch like me\n\n[Chorus]\nI once was lost but now am found\nWas blind but now I see"
	demo_song := parse_song("Demo Song", demo_src)

	// [dynamic]Song (not a fixed slice) so songs created in the editor can be
	// appended at runtime without invalidating existing playlists' indices.
	all_songs: [dynamic]Song
	defer {
		for &s in all_songs { song_free(&s) }
		delete(all_songs)
	}
	if len(library) > 0 {
		for s in library { append(&all_songs, s) }
		song_free(&demo_song)
	} else {
		append(&all_songs, demo_song)
	}

	playlists := load_playlists(PLAYLIST_FILE)
	defer playlists_free(&playlists)
	// Drop stale song indices left over from an earlier song library.
	for &pl in playlists {
		for i := len(pl.songs) - 1; i >= 0; i -= 1 {
			if pl.songs[i] < 0 || pl.songs[i] >= len(all_songs) { ordered_remove(&pl.songs, i) }
		}
	}

	song_idx      := 0
	section_idx   := 0
	slide_idx     := 0
	in_slide_mode := false
	active_playlist := 0
	if len(playlists[0].songs) > 0 { song_idx = playlists[0].songs[0] }
	// What's actually live on the beamer - only changes on a tile click or
	// arrow-key navigation of the song that's already live, never merely by
	// selecting a different song to browse.
	live_song_idx    := song_idx
	live_section_idx := section_idx

	naming_playlist   := false
	name_buf          := strings.builder_make()
	defer strings.builder_destroy(&name_buf)

	add_active     := false
	add_search_buf := strings.builder_make()
	defer strings.builder_destroy(&add_search_buf)

	library_active     := false
	library_search_buf := strings.builder_make()
	defer strings.builder_destroy(&library_search_buf)

	editor_active     := false
	editor_field      := Editor_Field.Title
	editor_cursor     := 0
	editor_anchor     := 0 // == editor_cursor when nothing is selected
	editor_dragging   := false
	editor_scroll     := 0 // first visible line index in the body textarea
	editor_edit_idx   := -1 // -1 = creating a new song, else editing all_songs[edit_idx]
	editor_title_buf  := strings.builder_make()
	defer strings.builder_destroy(&editor_title_buf)
	editor_body_buf   := strings.builder_make()
	defer strings.builder_destroy(&editor_body_buf)

	delete_pending_idx := -1 // armed delete confirmation in the library overlay

	help_active   := false
	last_ui_scale: f32 = 0

	show_all_active      := false
	show_all_song_idx    := -1
	show_all_cols        := i32(1)
	show_all_font:         ^ttf.Font = nil
	show_all_preview_font: ^ttf.Font = nil
	defer if show_all_font != nil { ttf.CloseFont(show_all_font) }
	defer if show_all_preview_font != nil { ttf.CloseFont(show_all_preview_font) }

	close_show_all :: proc(active: ^bool, song_idx: ^int, font, preview_font: ^^ttf.Font) {
		active^ = false
		song_idx^ = -1
		if font^ != nil { ttf.CloseFont(font^); font^ = nil }
		if preview_font^ != nil { ttf.CloseFont(preview_font^); preview_font^ = nil }
	}

	// section_text_buf is the browsing-cursor buffer: it gets freed and
	// reallocated every time the cursor moves, including while browsing a
	// song that isn't live. live_text_buf is a *separate* allocation that
	// content aliases - if content pointed directly at section_text_buf,
	// switching to browse another song would delete() the memory content
	// still references (use-after-free, showed up as garbage/tofu glyphs
	// on the beamer). Going live always clones into live_text_buf instead.
	section_text_buf := section_text(all_songs[song_idx].sections[0], context.allocator)
	defer delete(section_text_buf)
	live_text_buf := strings.clone(section_text_buf, context.allocator)
	defer delete(live_text_buf)
	content: Beamer_Content = live_text_buf

	// Moves the browsing cursor to sections[idx] (updates the text buffer so
	// it's ready to go live) without touching what's currently on the beamer.
	set_section_cursor :: proc(idx: int, sections: []Section, section_idx: ^int, buf: ^string) {
		if idx < 0 || idx >= len(sections) do return
		section_idx^ = idx
		delete(buf^)
		buf^ = section_text(sections[idx], context.allocator)
	}

	push_live_text :: proc(text: string, live_buf: ^string, content: ^Beamer_Content) {
		delete(live_buf^)
		live_buf^ = strings.clone(text, context.allocator)
		content^  = live_buf^
	}

	// Same, but also pushes to the beamer - used when the browsed song is
	// already the live one, or on an explicit tile click.
	push_section :: proc(idx: int, sections: []Section, section_idx: ^int, buf: ^string, live_buf: ^string, content: ^Beamer_Content) {
		set_section_cursor(idx, sections, section_idx, buf)
		if idx >= 0 && idx < len(sections) {
			push_live_text(buf^, live_buf, content)
		}
	}

	push_slide :: proc(idx: int, slides: []^sdl.Surface, slide_idx: ^int, content: ^Beamer_Content) {
		if idx < 0 || idx >= len(slides) do return
		slide_idx^ = idx
		content^   = slides[idx]
	}

	// Switching songs only updates what's shown in the tile grid - the
	// beamer keeps showing whatever was live until a tile is clicked (or
	// arrow-key navigation is used on the song that's already live).
	switch_song :: proc(
		new_idx:       int,
		all_songs:     []Song,
		song_idx:      ^int,
		section_idx:   ^int,
		buf:           ^string,
		in_slide_mode: ^bool,
	) {
		if new_idx < 0 || new_idx >= len(all_songs) do return
		song_idx^      = new_idx
		in_slide_mode^ = false
		set_section_cursor(0, all_songs[new_idx].sections, section_idx, buf)
	}

	op_w, op_h := i32(OPERATOR_W), i32(OPERATOR_H)
	running := true
	blanked := false
	event:   sdl.Event

	for running {
		sdl.GetWindowSize(op_win, &op_w, &op_h)
		scale := compute_scale(op_renderer, op_win)
		if scale != last_ui_scale {
			ttf.CloseFont(ui_font)
			ui_font = ttf.OpenFont(FONT_PATH, i32(f32(UI_FONT_SZ) * scale))
			last_ui_scale = scale
		}

		for sdl.PollEvent(&event) {
			#partial switch event.type {
			case .QUIT:
				running = false

			case .WINDOWEVENT:
				if event.window.event == .CLOSE { running = false }

			case .TEXTINPUT:
				if editor_active {
					target := &editor_title_buf if editor_field == .Title else &editor_body_buf
					t := event.text.text
					n := 0
					for n < 32 && t[n] != 0 { n += 1 }
					chunk := string(t[:n])
					if editor_cursor != editor_anchor {
						lo, hi := min(editor_cursor, editor_anchor), max(editor_cursor, editor_anchor)
						builder_delete_range(target, lo, hi)
						editor_cursor = lo
					}
					builder_insert_at(target, editor_cursor, chunk)
					editor_cursor += len(chunk)
					editor_anchor = editor_cursor
				} else {
					target: ^strings.Builder = nil
					if add_active      { target = &add_search_buf }
					else if library_active { target = &library_search_buf }
					else if naming_playlist { target = &name_buf }
					if target != nil {
						t := event.text.text
						for i in 0..<32 {
							if t[i] == 0 do break
							strings.write_byte(target, u8(t[i]))
						}
					}
				}

			case .MOUSEBUTTONDOWN:
				if event.button.button != 1 || event.button.windowID != op_id do break

				if help_active {
					help_active = false
					break
				}

				if editor_active {
					click, cur := editor_click(
						event.button.x, event.button.y,
						strings.to_string(editor_title_buf), strings.to_string(editor_body_buf),
						editor_scroll, ui_font, scale, op_w, op_h,
					)
					switch click {
					case .Field_Title, .Field_Body:
						new_field := Editor_Field.Title if click == .Field_Title else Editor_Field.Body
						text := strings.to_string(editor_title_buf) if new_field == .Title else strings.to_string(editor_body_buf)
						shift_held := (sdl.GetModState() & sdl.KMOD_SHIFT) != {}
						if event.button.clicks >= 2 {
							lo, hi := word_bounds_at(text, cur)
							editor_field = new_field
							editor_anchor = lo
							editor_cursor = hi
							editor_dragging = false
						} else if shift_held && editor_field == new_field {
							editor_cursor = cur
							editor_dragging = true
						} else {
							editor_field = new_field
							editor_anchor = cur
							editor_cursor = cur
							editor_dragging = true
						}
					case .Save:
						save_song_from_editor(&editor_active, &editor_title_buf, &editor_body_buf, &all_songs, editor_edit_idx)
					case .Cancel:
						editor_active = false
						sdl.StopTextInput()
					case .None:
					}
					break
				}

				if library_active {
					click, idx := library_overlay_click(
						event.button.x, event.button.y,
						all_songs[:], strings.to_string(library_search_buf), op_w, op_h,
					)
					if click != .Delete { delete_pending_idx = -1 }
					switch click {
					case .New_Song:
						library_active = false
						editor_active = true
						editor_field = .Title
						editor_cursor = 0
						editor_anchor = 0
						editor_scroll = 0
						editor_edit_idx = -1
						strings.builder_reset(&editor_title_buf)
						strings.builder_reset(&editor_body_buf)
					case .Edit:
						library_active = false
						editor_active = true
						editor_field = .Title
						editor_edit_idx = idx
						strings.builder_reset(&editor_title_buf)
						strings.write_string(&editor_title_buf, all_songs[idx].title)
						editor_cursor = len(strings.to_string(editor_title_buf))
						editor_anchor = editor_cursor
						editor_scroll = 0
						body := song_to_editor_text(all_songs[idx], context.temp_allocator)
						strings.builder_reset(&editor_body_buf)
						strings.write_string(&editor_body_buf, body)
					case .Delete:
						if delete_pending_idx == idx {
							delete_song(idx, &all_songs, &playlists)
							save_playlists(PLAYLIST_FILE, playlists[:])
							delete_pending_idx = -1
						} else {
							delete_pending_idx = idx
						}
					case .Close:
						library_active = false
						delete_pending_idx = -1
						sdl.StopTextInput()
					case .None:
					}
					break
				}

				if add_active {
					click, idx := add_overlay_click(
						event.button.x, event.button.y,
						all_songs[:], strings.to_string(add_search_buf), op_w, op_h,
					)
					switch click {
					case .Toggle:
						songs := &playlists[active_playlist].songs
						already := -1
						for s, pos in songs^ { if s == idx { already = pos; break } }
						if already >= 0 {
							remove_song_from_playlist(already, songs)
						} else {
							add_song_to_playlist(idx, songs)
							if len(songs^) == 1 {
								switch_song(idx, all_songs[:], &song_idx, &section_idx, &section_text_buf, &in_slide_mode)
							}
						}
						save_playlists(PLAYLIST_FILE, playlists[:])
					case .Close:
						add_active = false
						sdl.StopTextInput()
					case .None:
					}
					break
				}

				click, click_idx := panel_click(
					event.button.x, event.button.y,
					all_songs[:], song_idx, playlists[active_playlist].songs[:],
					len(playlists), len(all_songs[song_idx].sections),
					naming_playlist, ui_font, scale, op_w, op_h,
				)
				switch click {
				case .Setlist_Select:
					active_playlist = click_idx
					songs := playlists[active_playlist].songs
					if len(songs) > 0 {
						switch_song(songs[0], all_songs[:], &song_idx, &section_idx, &section_text_buf, &in_slide_mode)
					}
				case .Setlist_Remove:
					playlists_remove(&playlists, click_idx)
					if active_playlist >= len(playlists) { active_playlist = len(playlists) - 1 }
					save_playlists(PLAYLIST_FILE, playlists[:])
				case .Setlist_New:
					naming_playlist = true
					strings.builder_reset(&name_buf)
					sdl.StartTextInput()
				case .Song_Select:
					switch_song(click_idx, all_songs[:], &song_idx, &section_idx, &section_text_buf, &in_slide_mode)
				case .Song_Remove:
					remove_song_from_playlist(click_idx, &playlists[active_playlist].songs)
					save_playlists(PLAYLIST_FILE, playlists[:])
				case .Add_Open:
					add_active = true
					strings.builder_reset(&add_search_buf)
					sdl.StartTextInput()
				case .Library_Open:
					library_active = true
					delete_pending_idx = -1
					strings.builder_reset(&library_search_buf)
					sdl.StartTextInput()
				case .Tile:
					in_slide_mode = false
					push_section(click_idx, all_songs[song_idx].sections, &section_idx, &section_text_buf, &live_text_buf, &content)
					live_song_idx = song_idx
					live_section_idx = section_idx
					if show_all_active {
						close_show_all(&show_all_active, &show_all_song_idx, &show_all_font, &show_all_preview_font)
					}
				case .None:
				}

			case .MOUSEBUTTONUP:
				if event.button.windowID == op_id {
					editor_dragging = false
				}

			case .MOUSEMOTION:
				if editor_active && editor_dragging && event.motion.windowID == op_id {
					editor_cursor = editor_field_offset(
						event.motion.x, event.motion.y, editor_field,
						strings.to_string(editor_title_buf), strings.to_string(editor_body_buf),
						editor_scroll, ui_font, scale, op_w, op_h,
					)
				}

			case .MOUSEWHEEL:
				if editor_active && event.wheel.windowID == op_id {
					delta := event.wheel.y
					if event.wheel.direction == u32(sdl.MouseWheelDirection.FLIPPED) { delta = -delta }
					editor_scroll += int(delta)
					if editor_scroll < 0 { editor_scroll = 0 }
				}

			case .KEYDOWN:
				sym := event.key.keysym.sym
				if editor_active {
					cur_text := strings.to_string(editor_title_buf) if editor_field == .Title else strings.to_string(editor_body_buf)
					mod := event.key.keysym.mod
					cmd_held   := (mod & sdl.KMOD_GUI) != {}
					shift_held := (mod & sdl.KMOD_SHIFT) != {}
					alt_held   := (mod & sdl.KMOD_ALT) != {}
					has_sel := editor_cursor != editor_anchor
					sel_lo  := min(editor_cursor, editor_anchor)
					sel_hi  := max(editor_cursor, editor_anchor)
					target  := &editor_title_buf if editor_field == .Title else &editor_body_buf

					#partial switch sym {
					case .A:
						if cmd_held {
							editor_anchor = 0
							editor_cursor = len(cur_text)
						}
					case .V:
						if cmd_held {
							if text, ok := clipboard_paste_text(); ok {
								// Title is single-line: collapse pasted newlines to spaces.
								if editor_field == .Title {
									text, _ = strings.replace_all(text, "\n", " ", context.temp_allocator)
								}
								if has_sel {
									builder_delete_range(target, sel_lo, sel_hi)
									editor_cursor = sel_lo
								}
								builder_insert_at(target, editor_cursor, text)
								editor_cursor += len(text)
								editor_anchor = editor_cursor
							}
						}
					case .C:
						if cmd_held && has_sel {
							sdl.SetClipboardText(strings.clone_to_cstring(cur_text[sel_lo:sel_hi], context.temp_allocator))
						}
					case .X:
						if cmd_held && has_sel {
							sdl.SetClipboardText(strings.clone_to_cstring(cur_text[sel_lo:sel_hi], context.temp_allocator))
							builder_delete_range(target, sel_lo, sel_hi)
							editor_cursor = sel_lo
							editor_anchor = editor_cursor
						}
					case .TAB:
						editor_field = .Body if editor_field == .Title else .Title
						next_text := strings.to_string(editor_title_buf) if editor_field == .Title else strings.to_string(editor_body_buf)
						editor_cursor = len(next_text)
						editor_anchor = editor_cursor
					case .LEFT, .RIGHT, .HOME, .END:
						is_left := sym == .LEFT || sym == .HOME
						if !shift_held && has_sel && (sym == .LEFT || sym == .RIGHT) {
							editor_cursor = sel_lo if is_left else sel_hi
							editor_anchor = editor_cursor
						} else {
							new_pos := editor_cursor
							switch {
							case sym == .HOME:
								new_pos = line_start_boundary(cur_text, editor_cursor)
							case sym == .END:
								new_pos = line_end_boundary(cur_text, editor_cursor)
							case cmd_held && is_left:
								new_pos = line_start_boundary(cur_text, editor_cursor)
							case cmd_held:
								new_pos = line_end_boundary(cur_text, editor_cursor)
							case alt_held && is_left:
								new_pos = word_prev_boundary(cur_text, editor_cursor)
							case alt_held:
								new_pos = word_next_boundary(cur_text, editor_cursor)
							case is_left:
								new_pos = utf8_prev_boundary(cur_text, editor_cursor)
							case:
								new_pos = utf8_next_boundary(cur_text, editor_cursor)
							}
							editor_cursor = new_pos
							if !shift_held { editor_anchor = editor_cursor }
						}
					case .UP, .DOWN:
						if editor_field == .Body {
							dir := -1 if sym == .UP else 1
							editor_cursor = text_move_line(cur_text, editor_cursor, dir)
							if !shift_held { editor_anchor = editor_cursor }
						}
					case .BACKSPACE:
						if has_sel {
							builder_delete_range(target, sel_lo, sel_hi)
							editor_cursor = sel_lo
						} else {
							editor_cursor = builder_delete_before(target, editor_cursor)
						}
						editor_anchor = editor_cursor
					case .DELETE:
						if has_sel {
							builder_delete_range(target, sel_lo, sel_hi)
							editor_cursor = sel_lo
						} else {
							builder_delete_after(target, editor_cursor)
						}
						editor_anchor = editor_cursor
					case .RETURN:
						if editor_field == .Body {
							if has_sel {
								builder_delete_range(target, sel_lo, sel_hi)
								editor_cursor = sel_lo
							}
							builder_insert_at(target, editor_cursor, "\n")
							editor_cursor += 1
							editor_anchor = editor_cursor
						} else {
							editor_field = .Body
							editor_cursor = len(strings.to_string(editor_body_buf))
							editor_anchor = editor_cursor
						}
					case .ESCAPE:
						editor_active = false
						sdl.StopTextInput()
					}
				} else if add_active {
					#partial switch sym {
					case .BACKSPACE:
						builder_backspace(&add_search_buf)
					case .ESCAPE:
						add_active = false
						sdl.StopTextInput()
					}
				} else if library_active {
					#partial switch sym {
					case .BACKSPACE:
						builder_backspace(&library_search_buf)
					case .ESCAPE:
						library_active = false
						delete_pending_idx = -1
						sdl.StopTextInput()
					}
				} else if naming_playlist {
					#partial switch sym {
					case .BACKSPACE:
						builder_backspace(&name_buf)
					case .ESCAPE:
						naming_playlist = false
						sdl.StopTextInput()
					case .RETURN:
						name := strings.to_string(name_buf)
						final := name if len(name) > 0 else "Neue Setlist"
						append(&playlists, Playlist{name = strings.clone(final), songs = make([dynamic]int)})
						active_playlist = len(playlists) - 1
						save_playlists(PLAYLIST_FILE, playlists[:])
						naming_playlist = false
						sdl.StopTextInput()
					}
				} else {
					#partial switch sym {
					case .ESCAPE:
						if help_active { help_active = false } else { running = false }
					case .H:
						help_active = !help_active
					case .N:
						editor_active = true
						editor_field = .Title
						editor_cursor = 0
						editor_anchor = 0
						editor_scroll = 0
						editor_edit_idx = -1
						strings.builder_reset(&editor_title_buf)
						strings.builder_reset(&editor_body_buf)
						sdl.StartTextInput()
					case .S:
						if show_all_active {
							close_show_all(&show_all_active, &show_all_song_idx, &show_all_font, &show_all_preview_font)
						} else {
							show_all_active = true
						}
					case .BACKSPACE, .DELETE:
						for lib_idx, pos in playlists[active_playlist].songs {
							if lib_idx == song_idx {
								remove_song_from_playlist(pos, &playlists[active_playlist].songs)
								save_playlists(PLAYLIST_FILE, playlists[:])
								break
							}
						}
					case .B:
						blanked = !blanked
					case .RIGHT, .DOWN, .SPACE, .RETURN:
						if !in_slide_mode {
							if song_idx == live_song_idx {
								push_section(section_idx + 1, all_songs[song_idx].sections, &section_idx, &section_text_buf, &live_text_buf, &content)
								live_section_idx = section_idx
							} else {
								set_section_cursor(section_idx + 1, all_songs[song_idx].sections, &section_idx, &section_text_buf)
							}
						}
					case .LEFT, .UP:
						if !in_slide_mode {
							if song_idx == live_song_idx {
								push_section(section_idx - 1, all_songs[song_idx].sections, &section_idx, &section_text_buf, &live_text_buf, &content)
								live_section_idx = section_idx
							} else {
								set_section_cursor(section_idx - 1, all_songs[song_idx].sections, &section_idx, &section_text_buf)
							}
						}
					case .LEFTBRACKET:
						if len(slides) > 0 {
							in_slide_mode = true
							push_slide(slide_idx - 1, slides, &slide_idx, &content)
						}
					case .RIGHTBRACKET:
						if len(slides) > 0 {
							in_slide_mode = true
							push_slide(slide_idx + 1, slides, &slide_idx, &content)
						}
					case .L:
						in_slide_mode = false
						push_live_text(section_text_buf, &live_text_buf, &content)
						live_song_idx = song_idx
						live_section_idx = section_idx
						if show_all_active {
							close_show_all(&show_all_active, &show_all_song_idx, &show_all_font, &show_all_preview_font)
						}
					}
				}
			}
		}

		if show_all_active && show_all_song_idx != song_idx {
			if show_all_font != nil { ttf.CloseFont(show_all_font) }
			if show_all_preview_font != nil { ttf.CloseFont(show_all_preview_font) }
			fsize, cols := compute_show_all_layout(theme.font_path, all_songs[song_idx], bm.w, bm.h)
			show_all_cols = cols
			font_cs := strings.clone_to_cstring(theme.font_path, context.temp_allocator)
			show_all_font = ttf.OpenFont(font_cs, fsize)
			preview_fsize := max(i32(6), i32(f32(fsize) * f32(PREVIEW_H) / f32(bm.h)) * PREVIEW_SS)
			show_all_preview_font = ttf.OpenFont(font_cs, preview_fsize)
			show_all_song_idx = song_idx
		}

		render_operator(
			op_renderer, ui_font,
			all_songs[:], song_idx, section_idx, live_song_idx, live_section_idx,
			playlists[:], active_playlist, in_slide_mode,
			naming_playlist, strings.to_string(name_buf),
			preview_w, op_w, op_h, scale,
		)
		if len(playlists[active_playlist].songs) > 0 {
			if blanked {
				sdl.FillRect(preview_surf, nil, sdl.MapRGB(preview_surf.format, 0, 0, 0))
			} else if show_all_active && show_all_preview_font != nil {
				render_show_all_to_surface(preview_surf, preview_surf_w, preview_surf_h, show_all_preview_font, show_all_cols, all_songs[song_idx], theme.bg_r, theme.bg_g, theme.bg_b)
			} else {
				render_content_to_surface(preview_surf, preview_surf_w, preview_surf_h, preview_font, content, theme.bg_r, theme.bg_g, theme.bg_b)
			}
			preview_blit(op_renderer, preview_surf, preview_w, PREVIEW_H, op_w - preview_w - TILE_MARGIN, PREVIEW_BOX_Y, blanked, ui_font, scale)
		}
		if add_active {
			render_add_overlay(op_renderer, ui_font, all_songs[:], playlists[active_playlist].songs[:], strings.to_string(add_search_buf), op_w, op_h, scale)
		}
		if library_active {
			render_library_overlay(op_renderer, ui_font, all_songs[:], strings.to_string(library_search_buf), delete_pending_idx, op_w, op_h, scale)
		}
		if editor_active {
			render_editor_overlay(op_renderer, ui_font, strings.to_string(editor_title_buf), strings.to_string(editor_body_buf), editor_field, editor_cursor, editor_anchor, &editor_scroll, editor_edit_idx, op_w, op_h, scale)
		}
		if help_active {
			render_help_overlay(op_renderer, ui_font, op_w, op_h, scale)
		}
		sdl.RenderPresent(op_renderer)

		if blanked {
			beamer_clear(&bm, 0, 0, 0)
			beamer_present(&bm)
		} else if show_all_active && show_all_font != nil {
			render_show_all_to_surface(beamer_surface(&bm), bm.w, bm.h, show_all_font, show_all_cols, all_songs[song_idx], theme.bg_r, theme.bg_g, theme.bg_b)
			beamer_present(&bm)
		} else {
			beamer_render(&bm, beamer_font, content, theme.bg_r, theme.bg_g, theme.bg_b)
		}

		free_all(context.temp_allocator)
	}
}

builder_backspace :: proc(b: ^strings.Builder) {
	s := strings.to_string(b^)
	if len(s) == 0 do return
	i := len(s) - 1
	for i > 0 && (int(s[i]) & 0xC0) == 0x80 { i -= 1 }
	trimmed := s[:i]
	strings.builder_reset(b)
	strings.write_string(b, trimmed)
}
