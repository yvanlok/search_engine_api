# Use the official Rust image as a parent image
FROM rust:latest as builder

# Set the working directory inside the container
WORKDIR /usr/src/app

# Copy the Cargo.toml and Cargo.lock files to download dependencies efficiently
COPY Cargo.toml Cargo.lock ./

# Copy the rest of your source code to the working directory
COPY . .

# Build the application
RUN cargo build --release

# Create a new stage for the runtime image
FROM debian:buster-slim

# Set the working directory for the runtime image
WORKDIR /usr/src/app

# Copy the binary from the builder stage to the runtime image
COPY --from=builder /usr/src/app/target/release/search_engine_api .

# Optionally, copy your .env file
COPY .env ./.env

# Expose the port defined in your .env file
# Replace `PORT_NUMBER` with the variable name from your .env file
ENV PORT=$PORT_NUMBER
EXPOSE $PORT

# Command to run your application
CMD ["./search_engine_api"]
