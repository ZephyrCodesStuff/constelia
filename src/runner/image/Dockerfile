FROM python:3.13-alpine

# Optional: speed up installs, reduce image size
ENV PYTHONDONTWRITEBYTECODE=1
ENV PYTHONUNBUFFERED=1

# Install the base development tools (required for pwntools)
RUN apk add --no-cache \
    build-base \
    clang \
    cmake \
    g++ \
    gcc \
    git \
    musl-dev \
    linux-headers

WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Default entrypoint, will be overridden by the runner
ENTRYPOINT ["python3"]