#!/usr/bin/env sh
set -euo pipefail

VERSION=$(tomlq -r ".package.version" Cargo.toml)
PACKAGE=$(tomlq -r ".package.name" Cargo.toml)
for ARCH in x86_64-unknown-linux-gnu 
do
    cross build -r --target $ARCH
    cargo about generate about.hbs > licenses.html
    DEST=target/$PACKAGE-$VERSION-$ARCH.zip
    echo $DEST
    zip -j -r $DEST target/$ARCH/release/$PACKAGE target/$ARCH/release/ltapi-client licenses.html
    zip -sf $DEST
done
