package main

import "core:c"
import "core:fmt"
import "core:os"
import "core:path/filepath"
import "core:slice"
import "core:strings"
import sdl "vendor:sdl2"
import stbi "vendor:stb/image"

SLIDE_TMP_DIR :: "/tmp/gab_slides"

import_pptx :: proc(pptx_path: string) -> []^sdl.Surface {
	os.remove_all(SLIDE_TMP_DIR)
	os.make_directory(SLIDE_TMP_DIR)

	state1, _, _, err1 := os.process_exec(
		os.Process_Desc{command = {
			"soffice", "--headless",
			"--convert-to", "pdf",
			"--outdir", SLIDE_TMP_DIR,
			pptx_path,
		}},
		context.allocator,
	)
	if err1 != nil || state1.exit_code != 0 {
		fmt.eprintln("soffice conversion failed:", err1, "exit:", state1.exit_code)
		return nil
	}

	stem     := filepath.stem(pptx_path)
	pdf_name := strings.concatenate({stem, ".pdf"})
	defer delete(pdf_name)

	pdf_path, _ := filepath.join({SLIDE_TMP_DIR, pdf_name})
	defer delete(pdf_path)

	slide_prefix, _ := filepath.join({SLIDE_TMP_DIR, "slide"})
	defer delete(slide_prefix)

	state2, _, _, err2 := os.process_exec(
		os.Process_Desc{command = {
			"pdftoppm", "-r", "150", "-png",
			pdf_path, slide_prefix,
		}},
		context.allocator,
	)
	if err2 != nil || state2.exit_code != 0 {
		fmt.eprintln("pdftoppm failed:", err2, "exit:", state2.exit_code)
		return nil
	}

	entries, read_err := os.read_directory_by_path(SLIDE_TMP_DIR, -1, context.allocator)
	if read_err != nil {
		fmt.eprintln("read_dir failed:", read_err)
		return nil
	}
	defer os.file_info_slice_delete(entries, context.allocator)

	png_paths: [dynamic]string
	defer {
		for p in png_paths { delete(p) }
		delete(png_paths)
	}

	for entry in entries {
		if strings.has_prefix(entry.name, "slide") && strings.has_suffix(entry.name, ".png") {
			append(&png_paths, strings.clone(entry.fullpath))
		}
	}

	if len(png_paths) == 0 {
		fmt.eprintln("No slide PNGs generated")
		return nil
	}

	slice.sort(png_paths[:])

	surfaces := make([]^sdl.Surface, len(png_paths))
	for path, i in png_paths {
		surfaces[i] = load_png_surface(path)
	}

	fmt.printf("Imported %d slides from %s\n", len(surfaces), pptx_path)
	return surfaces
}

load_png_surface :: proc(path: string) -> ^sdl.Surface {
	cpath := strings.clone_to_cstring(path)
	defer delete(cpath)

	w, h, ch: c.int
	pixels := stbi.load(cpath, &w, &h, &ch, 4)
	if pixels == nil {
		fmt.eprintln("stbi load failed:", path)
		return nil
	}
	defer stbi.image_free(pixels)

	// CreateRGBSurfaceWithFormatFrom does NOT copy pixels; ConvertSurface makes a deep copy
	// so the surface owns its memory after stbi_image_free is called on return.
	tmp := sdl.CreateRGBSurfaceWithFormatFrom(
		pixels, w, h, 32, w * 4,
		u32(sdl.PixelFormatEnum.RGBA32),
	)
	if tmp == nil do return nil
	defer sdl.FreeSurface(tmp)
	return sdl.ConvertSurface(tmp, tmp.format, 0)
}
