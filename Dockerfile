FROM jimmycuadra/rust
RUN mkdir -p /code
WORKDIR /code
COPY . /code
EXPOSE 3000
CMD ["cargo", "run"]
