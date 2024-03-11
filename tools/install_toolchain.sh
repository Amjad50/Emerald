#!/bin/bash


# script used to install rust components into a prefix

prefix=$(realpath $1)
sysconfdir=$prefix/etc
if [ -z $prefix ]; then
    # panic
    echo "Usage: $0 <prefix> [file1.tar.xz file2.tar.xz ...]"
    exit 1
fi

# go through all the file names after the prefix
shift

# if there is no argument, print a warning
if [ $# -eq 0 ]; then
    echo "No components to install"
    exit 1
fi

for file in $@; do
    echo "Installing $file component"
    # make sure this is xz
    if [ ${file: -3} != ".xz" ]; then
        echo "File $file is not an xz file"
        exit 1
    fi
    # move into the folder of the xz file
    pushd $(dirname $file)
    # extract
    tar -xf $file
    # get the name of the directory
    dir=$(basename $file .tar.xz)
    # go into the directory
    pushd $dir
    # install
    sh install.sh --prefix=$prefix --sysconfdir=$sysconfdir
    # go back
    popd
    # remove the directory
    rm -r $dir
    # go back
    popd
done

