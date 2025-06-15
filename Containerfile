FROM rust:1

WORKDIR /usr/src/ephemeral
COPY . .

RUN cargo install --path .

ENV PORT=80
EXPOSE $PORT

CMD ["sh", "-c", "ephemeral ${PORT}"]