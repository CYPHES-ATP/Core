// Differential-test harness around Beam's reference BeamHash III code
// (BeamMW/beam 3rdparty/crypto/beamHashIII_impl.cpp, Apache-2.0).
//
// Mirrors beam::Block::PoW::Helper::Reset: Blake2b state = personalised
// base state, then input bytes, then the 8-byte nonce. Nothing here is
// used by the node; it only exists so tests can compare the Rust port
// against the code Beam's own network validates with.
//
//   beam3ref solve <input_hex> <nonce_hex>   print every valid solution
//   beam3ref verify                          stdin lines "input nonce sol" -> 1/0
#include "beamHashIII.h"
#include <cstdio>
#include <string>

static std::vector<uint8_t> unhex(const std::string& s) {
    std::vector<uint8_t> out;
    for (size_t i = 0; i + 1 < s.size(); i += 2)
        out.push_back((uint8_t) std::stoul(s.substr(i, 2), nullptr, 16));
    return out;
}

static std::string hex(const std::vector<uint8_t>& v) {
    static const char* d = "0123456789abcdef";
    std::string s;
    for (uint8_t b : v) { s += d[b >> 4]; s += d[b & 15]; }
    return s;
}

static blake2b_state state_for(BeamHash_III& pow, const std::vector<uint8_t>& input,
                               const std::vector<uint8_t>& nonce) {
    blake2b_state st;
    pow.InitialiseState(st);
    blake2b_update(&st, input.data(), input.size());
    blake2b_update(&st, nonce.data(), nonce.size());
    return st;
}

int main(int argc, char** argv) {
    BeamHash_III pow;
    std::string mode = argc > 1 ? argv[1] : "";
    if (mode == "solve" && argc == 4) {
        auto st = state_for(pow, unhex(argv[2]), unhex(argv[3]));
        int found = 0;
        pow.OptimisedSolve(
            st,
            [&](const std::vector<unsigned char>& sol) {
                // The reference solver does not filter duplicate-index
                // (trivial) solutions; only report what the verifier accepts.
                if (pow.IsValidSolution(st, sol)) {
                    std::printf("%s\n", hex(sol).c_str());
                    found++;
                }
                return false; // keep enumerating
            },
            [](SolverCancelCheck) { return false; });
        std::fprintf(stderr, "solutions: %d\n", found);
        return 0;
    }
    if (mode == "verify") {
        char a[4096], b[4096], c[4096];
        while (std::scanf("%4095s %4095s %4095s", a, b, c) == 3) {
            auto st = state_for(pow, unhex(a), unhex(b));
            std::printf("%d\n", pow.IsValidSolution(st, unhex(c)) ? 1 : 0);
        }
        return 0;
    }
    std::fprintf(stderr, "usage: beam3ref solve <input_hex> <nonce_hex> | beam3ref verify\n");
    return 2;
}
