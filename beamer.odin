package main

import "core:c"
import "core:strings"
import sdl "vendor:sdl2"
import ttf "vendor:sdl2/ttf"

Beamer :: struct {
	window: ^sdl.Window,
	w, h:   i32,
}

beamer_surface :: proc(b: ^Beamer) -> ^sdl.Surface {
	return sdl.GetWindowSurface(b.window)
}

beamer_present :: proc(b: ^Beamer) {
	sdl.UpdateWindowSurface(b.window)
}

beamer_clear :: proc(b: ^Beamer, r, g, bl: u8) {
	surf := beamer_surface(b)
	if surf == nil do return
	sdl.FillRect(surf, nil, sdl.MapRGB(surf.format, r, g, bl))
}

// Wrap a single line of text into multiple lines that fit within max_w pixels.
wrap_line :: proc(font: ^ttf.Font, line: string, max_w: i32, out: ^[dynamic]string) {
	if len(line) == 0 {
		append(out, "")
		return
	}

	// Fast path: whole line fits
	cs := strings.clone_to_cstring(line, context.temp_allocator)
	lw: c.int
	ttf.SizeUTF8(font, cs, &lw, nil)
	if i32(lw) <= max_w {
		append(out, strings.clone(line))
		return
	}

	// Split by spaces and build wrapped lines
	words := strings.split(line, " ", context.temp_allocator)
	current := strings.builder_make(context.temp_allocator)

	for word in words {
		sep := " " if strings.builder_len(current) > 0 else ""
		candidate := strings.concatenate({strings.to_string(current), sep, word}, context.temp_allocator)
		ccandidate := strings.clone_to_cstring(candidate, context.temp_allocator)
		cw: c.int
		ttf.SizeUTF8(font, ccandidate, &cw, nil)
		if i32(cw) > max_w && strings.builder_len(current) > 0 {
			append(out, strings.clone(strings.to_string(current)))
			strings.builder_reset(&current)
			strings.write_string(&current, word)
		} else {
			strings.write_string(&current, sep)
			strings.write_string(&current, word)
		}
	}
	if strings.builder_len(current) > 0 {
		append(out, strings.clone(strings.to_string(current)))
	}
}

// Renders centered text onto an arbitrary surface (the real beamer surface
// or an offscreen preview surface share this). Lines are never wrapped -
// if a line doesn't fit, the whole block is scaled down uniformly instead
// (BlitScaled), so a long line shrinks rather than breaking into two.
show_text_on_surface :: proc(surf: ^sdl.Surface, w, h: i32, font: ^ttf.Font, text: string, fg: sdl.Color) {
	if surf == nil do return

	line_h   := ttf.FontHeight(font)
	margin_x := w / 12
	margin_y := h / 12
	max_w    := w - margin_x * 2
	max_h    := h - margin_y * 2

	lines := strings.split(text, "\n", context.temp_allocator)

	widest := i32(0)
	for line in lines {
		if len(line) == 0 do continue
		cs := strings.clone_to_cstring(line, context.temp_allocator)
		lw: c.int
		ttf.SizeUTF8(font, cs, &lw, nil)
		if i32(lw) > widest { widest = i32(lw) }
	}
	if widest < 1 { widest = 1 }
	natural_h := line_h * i32(len(lines))
	if natural_h < 1 { natural_h = 1 }

	factor := min(f32(max_w) / f32(widest), f32(max_h) / f32(natural_h))
	factor = clamp(factor, 0.3, 1.0)

	scaled_line_h := i32(f32(line_h) * factor)
	total_h := scaled_line_h * i32(len(lines))
	y_start := margin_y

	for line, i in lines {
		if len(line) == 0 do continue
		cs := strings.clone_to_cstring(line, context.temp_allocator)
		text_surf := ttf.RenderUTF8_Blended(font, cs, fg)
		if text_surf == nil do continue
		defer sdl.FreeSurface(text_surf)

		dst_w := i32(f32(text_surf.w) * factor)
		dst_h := i32(f32(text_surf.h) * factor)
		x := (w - dst_w) / 2
		y := y_start + scaled_line_h * i32(i)
		dst := sdl.Rect{x = x, y = y, w = dst_w, h = dst_h}
		sdl.BlitScaled(text_surf, nil, surf, &dst)
	}
}

show_image_on_surface :: proc(surf: ^sdl.Surface, w, h: i32, img: ^sdl.Surface) {
	if surf == nil || img == nil do return
	dst := sdl.Rect{x = 0, y = 0, w = w, h = h}
	sdl.BlitScaled(img, nil, surf, &dst)
}

// Renders content (text or image slide) onto an arbitrary surface, used for
// both the real beamer surface and the operator-window preview thumbnail.
render_content_to_surface :: proc(surf: ^sdl.Surface, w, h: i32, font: ^ttf.Font, content: Beamer_Content, bg_r, bg_g, bg_b: u8) {
	if surf == nil do return
	sdl.FillRect(surf, nil, sdl.MapRGB(surf.format, bg_r, bg_g, bg_b))
	switch c in content {
	case ^sdl.Surface:
		show_image_on_surface(surf, w, h, c)
	case string:
		show_text_on_surface(surf, w, h, font, c, {255, 255, 255, 255})
	}
}

beamer_render :: proc(b: ^Beamer, font: ^ttf.Font, content: Beamer_Content, bg_r, bg_g, bg_b: u8) {
	surf := beamer_surface(b)
	render_content_to_surface(surf, b.w, b.h, font, content, bg_r, bg_g, bg_b)
	beamer_present(b)
}

// ── Show-all layout (whole song, multi-column, auto font size) ─────────────

section_kind_color :: proc(kind: Section_Kind) -> sdl.Color {
	switch kind {
	case .Chorus:    return {252, 211, 77, 255}
	case .Bridge:    return {244, 114, 182, 255}
	case .PreChorus: return {167, 139, 250, 255}
	case .Intro:     return {103, 232, 249, 255}
	case .Outro:     return {148, 163, 184, 255}
	case .Verse:     return {134, 239, 172, 255}
	}
	return {200, 200, 200, 255}
}

show_all_col_layout :: proc(w, h: i32, cols: i32) -> (margin_x, margin_y, gap, col_w: i32) {
	margin_x = w / 20
	margin_y = h / 20
	gap = w / 40
	col_w = (w - margin_x*2 - gap*(cols-1)) / cols
	return
}

// Measures whether the whole song fits in every column at the given font
// size, without actually drawing anything (used by the bisection search).
show_all_fits :: proc(font: ^ttf.Font, sections: []Section, cols: i32, col_w, avail_h: i32) -> bool {
	line_h := ttf.FontHeight(font)
	per_col := (i32(len(sections)) + cols - 1) / cols
	if per_col < 1 do per_col = 1

	for c in 0..<cols {
		total_h := i32(0)
		start := c * per_col
		end := min(start + per_col, i32(len(sections)))
		for i in start..<end {
			sec := sections[i]
			total_h += line_h
			wrapped: [dynamic]string
			for line in sec.lines { wrap_line(font, line, col_w, &wrapped) }
			total_h += line_h * i32(len(wrapped))
			for s in wrapped { delete(s) }
			delete(wrapped)
			total_h += line_h / 2
			if total_h > avail_h do return false
		}
	}
	return true
}

// Binary-searches the largest font size (4..160pt) at which the whole song,
// laid out in `cols` columns, fits within bm_w x bm_h.
bisect_show_all_size :: proc(font_path: string, sections: []Section, cols: i32, bm_w, bm_h: i32) -> i32 {
	_, _, _, col_w := show_all_col_layout(bm_w, bm_h, cols)
	margin_y := bm_h / 20
	avail_h := bm_h - margin_y*2

	cpath := strings.clone_to_cstring(font_path, context.temp_allocator)

	lo, hi := i32(4), i32(160)
	best := lo
	for lo <= hi {
		mid := (lo + hi) / 2
		font := ttf.OpenFont(cpath, mid)
		if font == nil { hi = mid - 1; continue }
		fits := show_all_fits(font, sections, cols, col_w, avail_h)
		ttf.CloseFont(font)
		if fits { best = mid; lo = mid + 1 } else { hi = mid - 1 }
	}
	return best
}

// Tries 1-3 columns and returns whichever yields the largest (most readable)
// font size, matching the layout strategy used by Erscheint's display.js.
// Sections split by a manual "---" break are recombined first, so show-all
// always presents whole verses, not individual slides.
compute_show_all_layout :: proc(font_path: string, song: Song, bm_w, bm_h: i32) -> (font_size: i32, cols: i32) {
	sections := merge_display_sections(song.sections)
	best_size := i32(0)
	best_cols := i32(1)
	for c in i32(1)..=3 {
		size := bisect_show_all_size(font_path, sections, c, bm_w, bm_h)
		if size > best_size {
			best_size = size
			best_cols = c
		}
	}
	return max(best_size, 10), best_cols
}

render_show_all_to_surface :: proc(surf: ^sdl.Surface, w, h: i32, font: ^ttf.Font, cols: i32, song: Song, bg_r, bg_g, bg_b: u8) {
	if surf == nil do return
	sdl.FillRect(surf, nil, sdl.MapRGB(surf.format, bg_r, bg_g, bg_b))

	sections := merge_display_sections(song.sections)
	margin_x, margin_y, gap, col_w := show_all_col_layout(w, h, cols)
	line_h := ttf.FontHeight(font)
	per_col := (i32(len(sections)) + cols - 1) / cols
	if per_col < 1 do per_col = 1
	labels := song_section_display_labels(sections)

	for c in 0..<cols {
		cx := margin_x + c*(col_w + gap)
		y := margin_y
		start := c * per_col
		end := min(start + per_col, i32(len(sections)))
		for i in start..<end {
			sec := sections[i]

			label_cs := strings.clone_to_cstring(labels[i], context.temp_allocator)
			if label_surf := ttf.RenderUTF8_Blended(font, label_cs, section_kind_color(sec.kind)); label_surf != nil {
				dst := sdl.Rect{x = cx, y = y, w = label_surf.w, h = label_surf.h}
				sdl.BlitSurface(label_surf, nil, surf, &dst)
				sdl.FreeSurface(label_surf)
			}
			y += line_h

			for line in sec.lines {
				wrapped: [dynamic]string
				wrap_line(font, line, col_w, &wrapped)
				for wl in wrapped {
					if len(wl) > 0 {
						wl_cs := strings.clone_to_cstring(wl, context.temp_allocator)
						if ls := ttf.RenderUTF8_Blended(font, wl_cs, sdl.Color{255, 255, 255, 255}); ls != nil {
							dst := sdl.Rect{x = cx, y = y, w = ls.w, h = ls.h}
							sdl.BlitSurface(ls, nil, surf, &dst)
							sdl.FreeSurface(ls)
						}
					}
					y += line_h
				}
				for wl in wrapped { delete(wl) }
				delete(wrapped)
			}
			y += line_h / 2
		}
	}
}
