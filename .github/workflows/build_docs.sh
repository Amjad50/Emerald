#!/bin/bash

set -eux pipefail

BASE_DIR="$(git rev-parse --show-toplevel)"

cd $BASE_DIR
cargo doc
(cd book
    (cd src && sed -e "s#{ROOT_PATH}#${1-/}#g" links.md.template > links.md)
    mdbook build
    mkdir -p book/docs
    cp -r ../target/doc/* book/docs/
)

