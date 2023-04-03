# mldec-rs

mldec-rs is a deserializer for compiled metalib binaries generated from the Tencent TDR protocol definition language.

This was made specifically to aid with reverse engineering a single specific game, and is not generic enough to be used for other projects.

(Based primarily on reverse engineering binaries w/ debug symbols of TDR [de]serialization logic)


# Usage
```bash
$ mldec <path to file containing compiled metalib> <starting offset in hex>
```
* Outputs to `./output/*.xml`