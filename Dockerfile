FROM rust:1.40

WORKDIR /usr/src/quinn-reverse-proxy
COPY Cargo.* ./
COPY src/main.rs src/main.rs
RUN cargo fetch

COPY . .
RUN cargo install --path .
EXPOSE 80

ENTRYPOINT ["quinn-reverse-proxy"]