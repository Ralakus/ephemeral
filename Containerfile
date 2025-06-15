FROM rust:1

WORKDIR /usr/src/ephemeral
COPY . .

RUN wget https://github.com/tailwindlabs/tailwindcss/releases/download/v4.1.10/tailwindcss-linux-x64 -O tailwindcss
RUN chmod +x tailwindcss
RUN cargo install --path .

ENV PORT=80
EXPOSE $PORT

CMD ["sh", "-c", "ephemeral ${PORT}"]