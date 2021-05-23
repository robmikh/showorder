# showorder
A utility to help order video files from TV shows using subtitles. Windows 10 only (uses Windows.Media.Ocr).

## Current status
Currently brittle and still requires some manual analysis.

## Usage
The tool requires the first parameter to be the path of an mkv with PGS subtitles (English) or of a directory with such mkv files in it. The second parameter is optional, and should either contain a srt file for a folder that contains srt files.

If only one parameter is supplied, the tool will print out the first 5 subtitles from each file. If both are provided, then the tool will attempt to match each file with a corresponding srt file.
