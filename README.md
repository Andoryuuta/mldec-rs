# mldec-rs

mldec-rs is a deserializer for compiled metalib binaries generated from the Tencent TDR protocol definition language.

This was made specifically to aid with reverse engineering a single specific game, and is not generic enough to be used for other projects.

(Based primarily on reverse engineering binaries w/ debug symbols of TDR [de]serialization logic)


# Usage
```bash
$ mldec <path to file containing compiled metalib> <starting offset in hex>
```
* Outputs to `./output/*.xml`

# Finding offset
Compiled metalibs usually start with the bytes `D6 02 0B 00 20`. Simply search your .exe/.dll binary for this pattern in a hex editor and try dumping the found file offsets.
