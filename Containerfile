FROM rust:1

WORKDIR /usr/src/ephemeral
COPY . .

RUN cargo install --path .
RUN wget https://github.com/tailwindlabs/tailwindcss/releases/download/v4.1.10/tailwindcss-linux-x64-musl -O tailwindcss

ENV PORT=80
EXPOSE $PORT

CMD ["sh", "-c", "ephemeral ${PORT}"]