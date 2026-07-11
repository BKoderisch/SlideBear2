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
	name:       string   `json:"name"`,
	songs:      []int    `json:"songs"`,
	song_paths: []string `json:"song_paths"`,
}

default_playlists :: proc(allocator := context.allocator) -> [dynamic]Playlist {
	result := make([dynamic]Playlist, allocator)
	append(&result, Playlist{name = strings.clone("Setlist", allocator), songs = make([dynamic]int, allocator)})
	return result
}

load_playlists :: proc(path: string, all_songs: []Song, allocator := context.allocator) -> [dynamic]Playlist {
	data, err := os.read_entire_file_from_path(path, context.temp_allocator)
	if err != nil do return default_playlists(allocator)
	defer delete(data, context.temp_allocator)

	raw: []Playlist_JSON
	if json.unmarshal(data, &raw, allocator = context.temp_allocator) != nil {
		return default_playlists(allocator)
	}
	if len(raw) == 0 do return default_playlists(allocator)

	path_to_idx := make(map[string]int, context.temp_allocator)
	for s, i in all_songs {
		if len(s.path) > 0 {
			path_to_idx[s.path] = i
		}
	}

	result := make([dynamic]Playlist, allocator)
	for p in raw {
		songs := make([dynamic]int, allocator)
		if len(p.song_paths) > 0 {
			for song_path in p.song_paths {
				idx, ok := path_to_idx[song_path]
				if ok {
					add_song_to_playlist(idx, &songs)
				}
			}
		}
		if len(songs) == 0 {
			for s in p.songs {
				if s >= 0 && s < len(all_songs) {
					add_song_to_playlist(s, &songs)
				}
			}
		}
		append(&result, Playlist{name = strings.clone(p.name, allocator), songs = songs})
	}
	return result
}

save_playlists :: proc(path: string, playlists: []Playlist, all_songs: []Song) {
	raw := make([]Playlist_JSON, len(playlists), context.temp_allocator)
	for p, i in playlists {
		song_paths := make([dynamic]string, context.temp_allocator)
		for song_idx in p.songs {
			if song_idx >= 0 && song_idx < len(all_songs) {
				song_path := all_songs[song_idx].path
				if len(song_path) > 0 {
					append(&song_paths, song_path)
				}
			}
		}
		raw[i] = Playlist_JSON{
			name       = p.name,
			songs      = p.songs[:],
			song_paths = song_paths[:],
		}
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
