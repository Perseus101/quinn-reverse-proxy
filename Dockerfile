FROM rust:1.36

WORKDIR /usr/src/quinn-reverse-proxy
COPY Cargo.* ./
COPY src/main.rs src/main.rs
RUN cargo fetch

COPY . .
RUN cargo install --path .

ENTRYPOINT ["quinn-reverse-proxy"]