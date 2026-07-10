package main

import "core:encoding/json"
import "core:fmt"
import "core:os"
import "core:strings"

PLAYLIST_FILE :: "playlists.json"

Playlist :: struct {
	name:  string,
	songs: [dynamic]int,
}

Playlist_JSON :: struct {
	name:  string `json:"name"`,
	songs: []int  `json:"songs"`,
}

default_playlists :: proc(allocator := context.allocator) -> [dynamic]Playlist {
	result := make([dynamic]Playlist, allocator)
	append(&result, Playlist{name = strings.clone("Setlist", allocator), songs = make([dynamic]int, allocator)})
	return result
}

load_playlists :: proc(path: string, allocator := context.allocator) -> [dynamic]Playlist {
	data, err := os.read_entire_file_from_path(path, context.temp_allocator)
	if err != nil do return default_playlists(allocator)
	defer delete(data, context.temp_allocator)

	raw: []Playlist_JSON
	if json.unmarshal(data, &raw, allocator = context.temp_allocator) != nil {
		return default_playlists(allocator)
	}
	if len(raw) == 0 do return default_playlists(allocator)

	result := make([dynamic]Playlist, allocator)
	for p in raw {
		songs := make([dynamic]int, allocator)
		for s in p.songs { append(&songs, s) }
		append(&result, Playlist{name = strings.clone(p.name, allocator), songs = songs})
	}
	return result
}

save_playlists :: proc(path: string, playlists: []Playlist) {
	raw := make([]Playlist_JSON, len(playlists), context.temp_allocator)
	for p, i in playlists {
		raw[i] = Playlist_JSON{name = p.name, songs = p.songs[:]}
	}
	data, err := json.marshal(raw, allocator = context.temp_allocator)
	if err != nil { fmt.eprintln("playlists.json marshal error:", err); return }
	if werr := os.write_entire_file(path, data); werr != nil {
		fmt.eprintln("playlists.json write failed:", werr)
	}
}

playlists_free :: proc(playlists: ^[dynamic]Playlist, allocator := context.allocator) {
	for &p in playlists {
		delete(p.name, allocator)
		delete(p.songs)
	}
	delete(playlists^)
}

add_song_to_playlist :: proc(idx: int, songs: ^[dynamic]int) {
	for existing in songs^ {
		if existing == idx do return
	}
	append(songs, idx)
}

// pos is the position within the playlist's song list, not an all_songs index.
remove_song_from_playlist :: proc(pos: int, songs: ^[dynamic]int) {
	if pos < 0 || pos >= len(songs) do return
	ordered_remove(songs, pos)
}

playlists_remove :: proc(playlists: ^[dynamic]Playlist, idx: int, allocator := context.allocator) {
	if idx < 0 || idx >= len(playlists) do return
	if len(playlists) <= 1 do return
	delete(playlists[idx].name, allocator)
	delete(playlists[idx].songs)
	ordered_remove(playlists, idx)
}
