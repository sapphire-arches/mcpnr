#!/usr/bin/env bash

my_dir=$(dirname $0)
cd ${my_dir}

CASES="
  adder
  counter
  nor
  nor-multibit
"

for c in ${CASES}
do
  pushd $c
  make
  popd
done
