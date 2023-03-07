# NOTE: check CWD!!

# only for debug
cargo run -- -t 2000 -x dict/ssh-clean.dict -i corpus/ -o out/ ./sshd -p 8989 -d -D -r -f none -h ssh_host_rsa_key -a @@
# with generator
cargo run -- -t 2000 -x dict/ssh-clean.dict -o out/ ./sshd -p 8989 -d -D -r -f none -h ssh_host_rsa_key -a @@

cargo run --release -- -s 784 --generate-count 1000 -t 2000 --stdout harness.out -x dict/ssh-clean.dict ./sshd -- -p 8989 -d -D -r -f none -h ssh_host_rsa_key -a @@
cargo run --release -- -s 784 -i corpus/ -t 2000 --stdout harness.out -x dict/ssh-clean.dict ./sshd -- -p 8989 -d -D -r -f none -h ssh_host_rsa_key -a @@