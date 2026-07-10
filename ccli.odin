package main

import "core:fmt"
import "core:os"
import "core:path/filepath"
import "core:strings"

// Parse a CCLI/SongSelect .txt export file.
//
// The format looks like:
//
//   Song Title
//
//   Author Name(s)
//
//   © Copyright info
//
//   CCLI Song # 1234567
//
//   Verse 1
//   Line one
//   Line two
//
//   Chorus
//   Refrain line
//
SECTION_KEYWORDS := []string{
	// English
	"verse", "chorus", "bridge", "pre-chorus", "prechorus",
	"intro", "outro", "tag", "interlude", "coda", "refrain", "ending",
	// German
	"vers", "strophe", "vorchor", "vorrefrain", "einleitung",
	"schluss", "ausklang", "zwischenspiel", "abschluss", "bruecke", "brücke",
}

// A line counts as a section header if it starts with one of `keywords`
// AND that match ends at a word boundary (end of line, space, digit,
// punctuation) - otherwise a lyric line like "Verschwende..." would
// false-positive match the short prefix "vers" and get swallowed as a label.
is_section_header :: proc(line: string, keywords: []string) -> bool {
	lower := strings.to_lower(line, context.temp_allocator)
	for kw in keywords {
		if !strings.has_prefix(lower, kw) do continue
		if len(lower) == len(kw) do return true
		next := lower[len(kw)]
		if next < 'a' || next > 'z' do return true
	}
	return false
}

parse_ccli_txt :: proc(src: string, allocator := context.allocator) -> (song: Song, ok: bool) {
	lines: [dynamic]string
	defer delete(lines)

	src := src
	for line in strings.split_lines_iterator(&src) {
		append(&lines, line)
	}

	if len(lines) == 0 do return song, false

	// First non-empty line is the title
	title := ""
	title_idx := 0
	for l, i in lines {
		t := strings.trim_space(l)
		if t != "" {
			title = t
			title_idx = i
			break
		}
	}

	section_keywords := SECTION_KEYWORDS

	// Collect sections starting after the header block
	// Header block = title + author + copyright + CCLI # lines (before first section header)
	sections: [dynamic]Section
	sections.allocator = allocator

	cur_label := ""
	cur_kind  := Section_Kind.Verse
	cur_lines: [dynamic]string
	cur_lines.allocator = allocator

	in_body := false
	// A blank line inside a section's lyrics is meaningful (a stanza gap
	// within the same slide) but a blank line just separating sections/
	// metadata isn't. Only materialize it once more real content follows,
	// so leading/trailing blanks around a header or "---" are discarded.
	pending_blank := false

	for i := title_idx + 1; i < len(lines); i += 1 {
		raw := lines[i]
		trimmed := strings.trim_space(raw)

		if !in_body {
			// Skip metadata until first section header or a second blank-then-text block
			if is_section_header(trimmed, section_keywords) {
				in_body = true
			} else if strings.has_prefix(strings.to_lower(trimmed, context.temp_allocator), "ccli") {
				// Skip CCLI license lines, next non-empty line starts content
				continue
			} else {
				// May still be metadata; keep skipping
				continue
			}
		}

		if is_section_header(trimmed, section_keywords) {
			pending_blank = false
			// Flush previous section
			if cur_label != "" && len(cur_lines) > 0 {
				append(&sections, Section{
					kind  = cur_kind,
					label = strings.clone(cur_label, allocator),
					lines = slice_clone_strings(cur_lines[:], allocator),
				})
				clear(&cur_lines)
			}
			cur_label = strings.clone(trimmed, context.temp_allocator)
			lower := strings.to_lower(trimmed, context.temp_allocator)
			switch {
			case strings.contains(lower, "chorus"):  cur_kind = .Chorus
			case strings.contains(lower, "bridge"):  cur_kind = .Bridge
			case strings.contains(lower, "pre"):     cur_kind = .PreChorus
			case strings.contains(lower, "intro"):   cur_kind = .Intro
			case strings.contains(lower, "outro"):   cur_kind = .Outro
			case strings.contains(lower, "tag"):     cur_kind = .Outro
			case:                                     cur_kind = .Verse
			}
		} else if trimmed == "---" {
			pending_blank = false
			// Manual slide break: flush what's accumulated as its own Section,
			// keep the same label/kind so the next lines continue as "part 2".
			if cur_label != "" && len(cur_lines) > 0 {
				append(&sections, Section{
					kind  = cur_kind,
					label = strings.clone(cur_label, allocator),
					lines = slice_clone_strings(cur_lines[:], allocator),
				})
				clear(&cur_lines)
			}
		} else if trimmed == "" {
			if len(cur_lines) > 0 { pending_blank = true }
		} else {
			if pending_blank {
				append(&cur_lines, strings.clone("", allocator))
				pending_blank = false
			}
			append(&cur_lines, strings.clone(trimmed, allocator))
		}
	}

	// Flush last section
	if cur_label != "" && len(cur_lines) > 0 {
		append(&sections, Section{
			kind  = cur_kind,
			label = strings.clone(cur_label, allocator),
			lines = slice_clone_strings(cur_lines[:], allocator),
		})
	}
	delete(cur_lines)

	if len(sections) == 0 {
		fmt.eprintln("CCLI parse: no sections found in", title)
		delete(sections)
		return song, false
	}

	return Song{
		title    = strings.clone(title, allocator),
		sections = sections[:],
	}, true
}

@(private)
slice_clone_strings :: proc(src: []string, allocator := context.allocator) -> []string {
	out := make([]string, len(src), allocator)
	for s, i in src { out[i] = strings.clone(s, allocator) }
	return out
}

// Scan a directory for .txt files, parse each as CCLI format.
load_song_library :: proc(dir: string, allocator := context.allocator) -> []Song {
	entries, err := os.read_directory_by_path(dir, -1, context.temp_allocator)
	if err != nil {
		fmt.eprintln("load_song_library: could not read", dir, err)
		return nil
	}

	songs: [dynamic]Song
	songs.allocator = allocator

	for entry in entries {
		if !strings.has_suffix(entry.name, ".txt") do continue

		data, ferr := os.read_entire_file_from_path(entry.fullpath, context.temp_allocator)
		if ferr != nil do continue

		src := string(data)
		if song, ok := parse_ccli_txt(src, allocator); ok {
			song.path = strings.clone(entry.fullpath, allocator)
			append(&songs, song)
			fmt.printf("Loaded song: %s (%d sections)\n", song.title, len(song.sections))
		}
	}

	return songs[:]
}
