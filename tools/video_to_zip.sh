# /bin/sh

# This script is used to convert a video file to a zip file

input_file=$1
output_file=$2
fps=$3
duration=$4

# exit if no input or output file is provided
if [ -z "$input_file" ] || [ -z "$output_file" ]; then
  echo "Usage: video_to_zip.sh <input_file> <output_file> [fps] [duration]"
  exit 1
fi

# check if input file exists
if [ ! -f "$input_file" ]; then
  echo "Input file does not exist"
  exit 1
fi

# create a temporary directory
temp_dir=$(mktemp -d)

if [ -z "$fps" ]; then
  fps=30
fi
if [ -z "$duration" ]; then
  duration="00:03:00"
fi

# extract the video file to the temporary directory
ffmpeg -i "$input_file" -t "$duration" -vf scale=640:-1 -r $fps/1 "$temp_dir/i%03d.jpg"

# remove the output file if it already exists
if [ -f "$output_file" ]; then
  rm "$output_file"
fi
# create a zip file from the temporary directory
zip -r -j "$output_file" "$temp_dir" > /dev/null

echo "Created $output_file, with size $(du -h $output_file | cut -f1)"

# remove the temporary directory
rm -r "$temp_dir"
