FROM golang:1.23-alpine AS builder

WORKDIR /app
COPY go.mod ./
RUN go mod download
COPY main.go ./
RUN CGO_ENABLED=0 go build -o echo-agent .

FROM alpine:3.20
RUN apk add --no-cache ca-certificates
COPY --from=builder /app/echo-agent /usr/local/bin/echo-agent
EXPOSE 9200
ENTRYPOINT ["echo-agent"]
