# Rust crate build stage
FROM rust:latest as builder
WORKDIR /usr/src/crosswords

# Copy Rust dependencies
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src

# Build the Rust program
RUN cargo install --path .

# Python runtime stage
FROM python:3.9-slim

# Set the working directory
WORKDIR /usr/app

# Copy the Rust binary from the builder stage
COPY --from=builder /usr/local/cargo/bin/crossword /usr/local/bin/crossword

# Install necessary system packages
RUN apt-get update && apt-get install -y \
    gcc \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy Python dependencies
COPY requirements.txt ./
COPY plot/requirements.txt ./plot/

# Install Python dependencies
RUN pip install --no-cache-dir -r requirements.txt

# Copy the application code
COPY run.py .
COPY plot/plot.py ./plot/plot.py

# Expose the port Flask will run on
EXPOSE 8080

# Set environment variables (these should be set in your environment or .env file)
ENV PORT=8080

# Command to run the Flask server with Gunicorn
CMD ["gunicorn", "--bind", "0.0.0.0:8080", "--workers", "1", "--threads", "8", "--timeout", "0", "--capture-output" "run:app"]

