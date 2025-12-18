# FromPlatformFlagConstDisallowed warning intentional - otherwise program ELF files differ when built on different host 
# platforms
FROM --platform=linux/amd64 rust:1.88 AS builder
WORKDIR /usr/src/sp1-lido-zk
# copying this file separately to avoid busting cache and rerunning bootstrap on every file change
COPY docker/docker_build_bootstrap.sh ./docker_build_bootstrap.sh
RUN ./docker_build_bootstrap.sh
# Install git for submodule initialization
RUN apt-get update && apt-get install -y git && rm -rf /var/lib/apt/lists/*
# See .dockerignore for list of copied files
COPY . .
ENV PATH="$PATH:/root/.sp1/bin:/root/.foundry/bin"
# Initialize git submodules for contract dependencies
RUN git config --global --add safe.directory /usr/src/sp1-lido-zk
RUN git submodule update --init --recursive --depth 1
ARG VERGEN_GIT_SHA
ENV VERGEN_GIT_SHA=${VERGEN_GIT_SHA}
RUN cargo build --release --locked
# this needs to be after build step above, to avoid cache-busting it
ARG PRINT_ELF_SHA
RUN echo "$PRINT_ELF_SHA" && sha256sum target/elf-compilation/riscv32im-succinct-zkvm-elf/release/sp1-lido-accounting-zk-program

# Cannot use alpine because we need glibc, and alpine uses musl. Between compiling for musl
# (only for docker), and using slightly larger base image, the latter seems a lesser evil
FROM --platform=linux/amd64 debian:stable-slim AS lido_sp1_oracle
WORKDIR /usr/data/sp1-lido-zk
RUN apt-get update && apt install -y openssl ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/sp1-lido-zk/target/release/service /usr/local/bin/service
COPY --from=builder /usr/src/sp1-lido-zk/target/release/deploy /usr/local/bin/deploy
COPY --from=builder /usr/src/sp1-lido-zk/target/release/store_report /usr/local/bin/store_report
COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
COPY docker/healthcheck.sh /usr/local/bin/healthcheck.sh
RUN chmod +x /usr/local/bin/healthcheck.sh
RUN chmod +x /usr/local/bin/entrypoint.sh
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["service"]
HEALTHCHECK --interval=30s --timeout=20s --start-period=5s --retries=3 CMD /usr/local/bin/healthcheck.sh