#!/bin/bash

cargo build --release
cd frontend
npm run build
cd ..

rm -rf release
mkdir release
mkdir release/frontend

cp -r ./target/release/ruler ./release
cp -r ./frontend/build/ ./release/frontend

rm ruler.7z
7z a ruler.7z release

