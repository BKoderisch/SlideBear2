package main

import "base:runtime"
import "core:fmt"
import "core:slice"
import "core:strings"

Section_Kind :: enum {
	Verse,
	Chorus,
	Bridge,
	PreChorus,
	Outro,
	Intro,
}

Section :: struct {
	kind:  Section_Kind,
	label: string,
	lines: []string,
}

Song :: struct {
	title:    string,
	sections: []Section,
	path:     string, // source .txt file in songs/; empty for the built-in demo song
	deleted:  bool,   // soft-deleted: hidden from listings, kept in place so
	                  // playlist indices referencing later songs stay valid
}

section_text :: proc(s: Section, allocator := context.allocator) -> string {
	return strings.join(s.lines, "\n", allocator)
}

// A manual "---" break inside a section (see parse_ccli_txt / parse_song)
// splits it into multiple Section entries that share the same label, so a
// long verse can become several slides. Returns per-index display labels
// with a "(part/total)" suffix added wherever a label repeats, purely for
// rendering (tile headers), the stored labels themselves stay identical.
song_section_display_labels :: proc(sections: []Section) -> []string {
	out := make([]string, len(sections), context.temp_allocator)
	total := make(map[string]int, context.temp_allocator)
	for sec in sections { total[sec.label] += 1 }
	seen := make(map[string]int, context.temp_allocator)
	for sec, i in sections {
		if total[sec.label] > 1 {
			seen[sec.label] += 1
			out[i] = fmt.tprintf("%s (%d/%d)", sec.label, seen[sec.label], total[sec.label])
		} else {
			out[i] = sec.label
		}
	}
	return out
}

// The inverse of a "---" split: recombines consecutive sections that share
// a label/kind back into one logical section (blank line where the split
// was), so the show-all view presents a verse that was split into multiple
// slides as a single continuous verse again.
merge_display_sections :: proc(sections: []Section, allocator := context.temp_allocator) -> []Section {
	if len(sections) == 0 do return nil
	out: [dynamic]Section
	out.allocator = allocator

	cur_kind := sections[0].kind
	cur_label := sections[0].label
	cur_lines: [dynamic]string
	cur_lines.allocator = allocator
	for line in sections[0].lines { append(&cur_lines, line) }

	for i in 1..<len(sections) {
		sec := sections[i]
		if sec.label == cur_label && sec.kind == cur_kind {
			append(&cur_lines, "")
			for line in sec.lines { append(&cur_lines, line) }
		} else {
			append(&out, Section{kind = cur_kind, label = cur_label, lines = cur_lines[:]})
			cur_kind = sec.kind
			cur_label = sec.label
			cur_lines = make([dynamic]string, allocator)
			for line in sec.lines { append(&cur_lines, line) }
		}
	}
	append(&out, Section{kind = cur_kind, label = cur_label, lines = cur_lines[:]})
	return out[:]
}

@(private)
flush_section :: proc(
	sections: ^[dynamic]Section,
	kind: Section_Kind,
	label: string,
	lines: ^[dynamic]string,
	allocator: runtime.Allocator,
) {
	if len(lines) == 0 do return
	append(sections, Section{
		kind  = kind,
		label = strings.clone(label, allocator),
		lines = slice.clone(lines[:], allocator),
	})
	clear(lines)
}

// Parse plain-text song format:
//
//   [Verse 1]
//   Line one
//   Line two
//
//   [Chorus]
//   Hallelujah
//
parse_song :: proc(title: string, src: string, allocator := context.allocator) -> Song {
	sections: [dynamic]Section
	sections.allocator = allocator

	current_kind  := Section_Kind.Verse
	current_label := ""
	current_lines: [dynamic]string
	current_lines.allocator = allocator

	src := src
	for raw_line in strings.split_lines_iterator(&src) {
		line := strings.trim_space(raw_line)
		if line == "" do continue

		if strings.has_prefix(line, "[") && strings.has_suffix(line, "]") {
			flush_section(&sections, current_kind, current_label, &current_lines, allocator)
			current_label = strings.trim(line, "[]")
			lower := strings.to_lower(current_label, context.temp_allocator)
			switch {
			case strings.contains(lower, "chorus"):   current_kind = .Chorus
			case strings.contains(lower, "bridge"):   current_kind = .Bridge
			case strings.contains(lower, "pre"):      current_kind = .PreChorus
			case strings.contains(lower, "outro"):    current_kind = .Outro
			case strings.contains(lower, "intro"):    current_kind = .Intro
			case:                                      current_kind = .Verse
			}
		} else if line == "---" {
			// Manual slide break: flush what's accumulated as its own Section,
			// keep the same label/kind so the next lines continue as "part 2".
			flush_section(&sections, current_kind, current_label, &current_lines, allocator)
		} else {
			append(&current_lines, strings.clone(line, allocator))
		}
	}

	flush_section(&sections, current_kind, current_label, &current_lines, allocator)
	delete(current_lines)

	return Song{
		title    = strings.clone(title, allocator),
		sections = sections[:],
	}
}

song_free :: proc(song: ^Song, allocator := context.allocator) {
	for &sec in song.sections {
		delete(sec.label, allocator)
		for line in sec.lines { delete(line, allocator) }
		delete(sec.lines, allocator)
	}
	delete(song.sections, allocator)
	delete(song.title, allocator)
	delete(song.path, allocator)
}
