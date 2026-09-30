#!/bin/sh
# Builds the Beam reference harness.
#
#   git clone https://github.com/BeamMW/beam
#   BEAM_SRC=/path/to/beam ./build.sh [output]
set -eu
: "${BEAM_SRC:?set BEAM_SRC to a BeamMW/beam checkout}"
OUT="${1:-$(dirname "$0")/beam3ref}"
C="$BEAM_SRC/3rdparty/crypto"
cc -O3 -c "$C/blake/ref/blake2b-ref.c" -I"$C/blake/ref" -o "${OUT}.blake2b.o"
c++ -O3 -std=c++17 -DENABLE_MINING -I"$C" -I"$C/blake/ref" \
    "$(dirname "$0")/beam3ref.cpp" "$C/beamHashIII_impl.cpp" "${OUT}.blake2b.o" -o "$OUT"
rm -f "${OUT}.blake2b.o"
echo "built $OUT"
