package main

import "core:c"
import "core:fmt"
import "core:os"
import "core:strings"
import sdl "vendor:sdl2"
import ttf "vendor:sdl2/ttf"

// ── Clipboard (Cmd+V / Cmd+C in the editor's text fields) ──────────────────

clipboard_paste_text :: proc() -> (text: string, ok: bool) {
	if !sdl.HasClipboardText() do return "", false
	cstr := sdl.GetClipboardText()
	if cstr == nil do return "", false
	raw := string(cstr)
	normalized, _ := strings.replace_all(raw, "\r\n", "\n", context.temp_allocator)
	normalized, _ = strings.replace_all(normalized, "\r", "\n", context.temp_allocator)
	return normalized, true
}

// ── Cursor-aware text buffer editing (title field + body textarea) ─────────

utf8_prev_boundary :: proc(s: string, i: int) -> int {
	j := i
	if j <= 0 do return 0
	j -= 1
	for j > 0 && (int(s[j]) & 0xC0) == 0x80 { j -= 1 }
	return j
}

utf8_next_boundary :: proc(s: string, i: int) -> int {
	j := i
	if j >= len(s) do return len(s)
	j += 1
	for j < len(s) && (int(s[j]) & 0xC0) == 0x80 { j += 1 }
	return j
}

is_word_byte :: proc(b: u8) -> bool {
	return b != ' ' && b != '\t' && b != '\n'
}

// Option/Alt+Left: skip whitespace, then skip the word, landing at its start.
word_prev_boundary :: proc(s: string, i: int) -> int {
	j := i
	for j > 0 && !is_word_byte(s[j-1]) { j -= 1 }
	for j > 0 && is_word_byte(s[j-1]) { j -= 1 }
	return j
}

// Option/Alt+Right: skip the word, then skip whitespace, landing at the next word.
word_next_boundary :: proc(s: string, i: int) -> int {
	j := i
	for j < len(s) && is_word_byte(s[j]) { j += 1 }
	for j < len(s) && !is_word_byte(s[j]) { j += 1 }
	return j
}

line_start_boundary :: proc(s: string, i: int) -> int {
	j := i
	for j > 0 && s[j-1] != '\n' { j -= 1 }
	return j
}

line_end_boundary :: proc(s: string, i: int) -> int {
	j := i
	for j < len(s) && s[j] != '\n' { j += 1 }
	return j
}

builder_insert_at :: proc(b: ^strings.Builder, at: int, text: string) {
	s := strings.to_string(b^)
	at := clamp(at, 0, len(s))
	combined := strings.concatenate({s[:at], text, s[at:]}, context.temp_allocator)
	strings.builder_reset(b)
	strings.write_string(b, combined)
}

// Deletes the codepoint immediately before `at`. Returns the new cursor position.
builder_delete_before :: proc(b: ^strings.Builder, at: int) -> int {
	s := strings.to_string(b^)
	if at <= 0 do return 0
	prev := utf8_prev_boundary(s, at)
	combined := strings.concatenate({s[:prev], s[at:]}, context.temp_allocator)
	strings.builder_reset(b)
	strings.write_string(b, combined)
	return prev
}

// Deletes the codepoint immediately after `at` (Delete/Entf key).
builder_delete_after :: proc(b: ^strings.Builder, at: int) {
	s := strings.to_string(b^)
	if at >= len(s) do return
	nxt := utf8_next_boundary(s, at)
	combined := strings.concatenate({s[:at], s[nxt:]}, context.temp_allocator)
	strings.builder_reset(b)
	strings.write_string(b, combined)
}

// Deletes [from, to) (order-independent). Used for selection delete/replace.
builder_delete_range :: proc(b: ^strings.Builder, from, to: int) {
	s := strings.to_string(b^)
	lo := clamp(min(from, to), 0, len(s))
	hi := clamp(max(from, to), 0, len(s))
	if lo == hi do return
	combined := strings.concatenate({s[:lo], s[hi:]}, context.temp_allocator)
	strings.builder_reset(b)
	strings.write_string(b, combined)
}

// Moves the cursor up (dir<0) or down (dir>0) one line in a multi-line
// buffer, keeping roughly the same column, for the body textarea.
text_move_line :: proc(s: string, cursor: int, dir: int) -> int {
	line_start := cursor
	for line_start > 0 && s[line_start-1] != '\n' { line_start -= 1 }
	col := cursor - line_start

	if dir < 0 {
		if line_start == 0 do return cursor
		prev_end := line_start - 1
		prev_start := prev_end
		for prev_start > 0 && s[prev_start-1] != '\n' { prev_start -= 1 }
		return prev_start + min(col, prev_end - prev_start)
	}

	line_end := cursor
	for line_end < len(s) && s[line_end] != '\n' { line_end += 1 }
	if line_end >= len(s) do return cursor
	next_start := line_end + 1
	next_end := next_start
	for next_end < len(s) && s[next_end] != '\n' { next_end += 1 }
	return next_start + min(col, next_end - next_start)
}

// Finds the byte offset within `line` whose character boundary is closest
// to on-screen x position target_x (point-space, matching render_text_line's
// scale-corrected destination width).
text_offset_for_x :: proc(font: ^ttf.Font, line: string, target_x: i32, scale: f32) -> int {
	if target_x <= 0 || len(line) == 0 do return 0
	prev_w := i32(0)
	i := 0
	for i < len(line) {
		next := utf8_next_boundary(line, i)
		cs := strings.clone_to_cstring(line[:next], context.temp_allocator)
		w: c.int
		ttf.SizeUTF8(font, cs, &w, nil)
		scaled_w := i32(f32(w) / scale)
		mid := (prev_w + scaled_w) / 2
		if target_x < mid do return i
		prev_w = scaled_w
		i = next
	}
	return len(line)
}

// Point-space x offset of byte position `at` within `line`, i.e. the pixel
// width of line[:at] - used to place the caret and selection highlight.
text_x_for_offset :: proc(font: ^ttf.Font, line: string, at: int, scale: f32) -> i32 {
	off := clamp(at, 0, len(line))
	if off == 0 do return 0
	cs := strings.clone_to_cstring(line[:off], context.temp_allocator)
	w: c.int
	ttf.SizeUTF8(font, cs, &w, nil)
	return i32(f32(w) / scale)
}

// Word boundaries around byte offset `at`, for double-click word selection.
word_bounds_at :: proc(s: string, at: int) -> (lo, hi: int) {
	at := clamp(at, 0, len(s))
	lo = at
	for lo > 0 && is_word_byte(s[lo-1]) { lo -= 1 }
	hi = at
	for hi < len(s) && is_word_byte(s[hi]) { hi += 1 }
	if lo == hi {
		// clicked on whitespace/an empty gap: fall back to a single codepoint
		hi = utf8_next_boundary(s, lo)
	}
	return
}

slugify :: proc(title: string, allocator := context.allocator) -> string {
	b := strings.builder_make(allocator)
	prev_us := false
	for r in title {
		switch {
		case r >= 'A' && r <= 'Z':
			strings.write_rune(&b, r + 32)
			prev_us = false
		case r >= 'a' && r <= 'z', r >= '0' && r <= '9':
			strings.write_rune(&b, r)
			prev_us = false
		case:
			if !prev_us {
				strings.write_byte(&b, '_')
				prev_us = true
			}
		}
	}
	return strings.trim(strings.to_string(b), "_")
}

// Writes song text (same plain-text format the existing library files use:
// title line, blank line, then bare section headers like "Verse 1" / "Chorus"
// understood by parse_ccli_txt) to an exact path, overwriting it if present.
save_song_file :: proc(path: string, title: string, body: string) -> bool {
	content := fmt.tprintf("%s\n\n%s\n", title, body)
	if err := os.write_entire_file(path, content); err != nil {
		fmt.eprintln("save_song_file write failed:", err)
		return false
	}
	return true
}

// Picks a free songs/<slug>.txt path and writes a brand-new song file there.
save_new_song :: proc(title: string, body: string) -> (path: string, ok: bool) {
	slug := slugify(title, context.temp_allocator)
	if len(slug) == 0 do slug = "song"

	candidate := fmt.tprintf("songs/%s.txt", slug)
	n := 2
	for os.exists(candidate) {
		candidate = fmt.tprintf("songs/%s_%d.txt", slug, n)
		n += 1
	}

	ok = save_song_file(candidate, title, body)
	return candidate, ok
}

// Reconstructs the editor's plain-text body format from a parsed Song, so
// editing an existing song can round-trip through the same textarea format
// used to create new ones.
song_to_editor_text :: proc(song: Song, allocator := context.allocator) -> string {
	b := strings.builder_make(allocator)
	for sec, i in song.sections {
		if i == 0 {
			strings.write_string(&b, sec.label)
		} else {
			prev := song.sections[i-1]
			// Consecutive sections with the same label/kind came from a
			// manual "---" slide break - reconstruct that marker instead of
			// writing the header again, so a re-edited song keeps being one
			// verse split into slides rather than turning into two verses.
			if sec.label == prev.label && sec.kind == prev.kind {
				strings.write_string(&b, "\n---")
			} else {
				strings.write_string(&b, "\n\n")
				strings.write_string(&b, sec.label)
			}
		}
		for line in sec.lines {
			strings.write_byte(&b, '\n')
			strings.write_string(&b, line)
		}
	}
	return strings.to_string(b)
}

// parse_ccli_txt only starts capturing lyrics once it sees a line matching
// SECTION_KEYWORDS ("Verse 1", "Chorus", ...) - without one it silently
// discards the whole song (0 sections). The editor doesn't force users to
// know that convention, so prepend a default header if none is present.
ensure_body_has_header :: proc(body: string) -> string {
	rest := body
	for {
		idx := strings.index_byte(rest, '\n')
		line := rest if idx == -1 else rest[:idx]
		trimmed := strings.trim_space(line)
		if trimmed != "" {
			if is_section_header(trimmed, SECTION_KEYWORDS) do return body
			break
		}
		if idx == -1 do break
		rest = rest[idx+1:]
	}
	return fmt.tprintf("Strophe 1\n%s", body)
}

// edit_idx < 0 creates a new song and appends it; edit_idx >= 0 overwrites
// the existing file and replaces all_songs[edit_idx] in place, so playlist
// indices into all_songs stay valid either way.
save_song_from_editor :: proc(
	editor_active: ^bool,
	title_buf, body_buf: ^strings.Builder,
	all_songs: ^[dynamic]Song,
	edit_idx: int,
) {
	title := strings.to_string(title_buf^)
	body  := ensure_body_has_header(strings.to_string(body_buf^))
	if len(title) == 0 || len(body) == 0 do return

	full := fmt.tprintf("%s\n\n%s\n", title, body)

	if edit_idx >= 0 && edit_idx < len(all_songs) {
		path := all_songs[edit_idx].path
		if !save_song_file(path, title, body) do return
		if song, parsed := parse_ccli_txt(full, context.allocator); parsed {
			song.path = strings.clone(path)
			song_free(&all_songs[edit_idx])
			all_songs[edit_idx] = song
		}
	} else {
		path, ok := save_new_song(title, body)
		if !ok do return
		if song, parsed := parse_ccli_txt(full, context.allocator); parsed {
			song.path = strings.clone(path)
			append(all_songs, song)
		}
	}

	editor_active^ = false
	sdl.StopTextInput()
}

// Soft-deletes: removes the file, drops the song from every playlist, and
// marks it hidden without shifting all_songs (which would break every other
// playlist's stored indices).
delete_song :: proc(idx: int, all_songs: ^[dynamic]Song, playlists: ^[dynamic]Playlist) {
	if idx < 0 || idx >= len(all_songs) do return
	song := &all_songs[idx]
	if song.deleted do return

	if len(song.path) > 0 {
		if err := os.remove(song.path); err != nil {
			fmt.eprintln("delete_song remove failed:", err)
		}
	}
	song.deleted = true

	for &pl in playlists {
		for i := len(pl.songs) - 1; i >= 0; i -= 1 {
			if pl.songs[i] == idx { ordered_remove(&pl.songs, i) }
		}
	}
}
