#!/bin/bash
set -e
git subtree pull --prefix=minix3 \
  https://github.com/Stichting-MINIX-Research-Foundation/minix.git \
  master --squash
echo "Minix3 updated in ./minix3/"