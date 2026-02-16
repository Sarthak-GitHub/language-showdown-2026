FROM golang:1.23-alpine AS builder
WORKDIR /app
COPY go.mod go.sum ./
RUN go mod download
COPY main.go .
RUN CGO_ENABLED=0 GOOS=linux go build -o api main.go

FROM alpine:latest
WORKDIR /app
COPY --from=builder /app/api .
EXPOSE 8001
CMD ["./api"]
