#!/bin/bash

# install a zipped/folder toolchain into the prefix found in `<this_file>/../extern/toolchain`
# and link it with `rustup` to toolchain `emerald`

zip_file=$1
if [ -z $zip_file ]; then
    echo "Usage: $0 <zip_file/folder>"
    exit 1
fi

if [ -f $zip_file ]; then
    # check zip extension
    if [ ${zip_file: -4} != ".zip" ]; then
        echo "File $zip_file is not a zip file"
        exit 1
    fi

    # extract the zip file into temp
    temp_dir=$(mktemp -d)
    unzip -q $zip_file -d $temp_dir
else
    # check if it is a directory
    if [ ! -d $zip_file ]; then
        echo "File $zip_file does not exist"
        exit 1
    fi
fi


# get the directory of this file
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
# get the prefix
prefix=$DIR/../extern/toolchain

if [ -d $zip_file ]; then
    temp_dir=$zip_file
fi

echo "installing from $temp_dir"


# install the toolchain
bash $DIR/install_toolchain.sh $prefix $temp_dir/*.xz

# link the toolchain
rustup toolchain link emerald $prefix

# remove the temp directory
if [ ! -d $zip_file ]; then
    rm -r $temp_dir
fi

echo "Installed and linked \"$zip_file\" into \"$prefix\" and linked it with rustup as toolchain \`emerald\`"
echo "You can use it with \`cargo +emerald build\`"
