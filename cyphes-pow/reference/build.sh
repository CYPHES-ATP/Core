#!/bin/sh
# Builds the Beam reference harness. BEAM_SRC points at a BeamMW/beam checkout.
set -eu
BEAM_SRC="${BEAM_SRC:-$HOME/Desktop/CYPHES/upstream/beam}"
OUT="${1:-$(dirname "$0")/beam3ref}"
C="$BEAM_SRC/3rdparty/crypto"
cc -O3 -c "$C/blake/ref/blake2b-ref.c" -I"$C/blake/ref" -o "${OUT}.blake2b.o"
c++ -O3 -std=c++17 -DENABLE_MINING -I"$C" -I"$C/blake/ref" \
    "$(dirname "$0")/beam3ref.cpp" "$C/beamHashIII_impl.cpp" "${OUT}.blake2b.o" -o "$OUT"
rm -f "${OUT}.blake2b.o"
echo "built $OUT"
