default:
    @just build --release
    @just cp

cp:
    rsync -Paz -e "ssh -i ~/.ssh/gcp" /home/ae/repos/pzcli/target/x86_64-unknown-linux-gnu/release/pzcli ae@34.51.17.167:pzcli

[positional-arguments]
build *flags='':
    env RUSTFLAGS=-Awarnings cargo zigbuild -q --target x86_64-unknown-linux-gnu.2.39 $@
