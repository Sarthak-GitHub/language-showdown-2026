package main

import (
	"context"
	"crypto/sha256"
	"database/sql"
	"encoding/json"
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/gofiber/fiber/v2"
	"github.com/gofiber/fiber/v2/middleware/requestid"
	"github.com/golang-jwt/jwt/v5"
	"github.com/jackc/pgx/v5/stdlib"
	"github.com/redis/go-redis/v9"
	"github.com/rs/zerolog"
	"github.com/rs/zerolog/log"
)

var (
	secretKey     = []byte("super-secret-key-for-benchmark-only")
	rdb           *redis.Client
	db            *sql.DB
	rateLimit     = 100
	rateWindowSec = 60
)

type User struct {
	ID    int    `json:"id"`
	Name  string `json:"name"`
	Email string `json:"email"`
}

type Claims struct {
	Sub string `json:"sub"`
	jwt.RegisteredClaims
}

func main() {
	zerolog.TimeFieldFormat = zerolog.TimeFormatUnix
	log.Logger = log.Output(zerolog.ConsoleWriter{Out: os.Stderr})

	var err error
	db, err = sql.Open("pgx", "postgres://benchmark:benchmark@localhost:5432/benchmark?sslmode=disable")
	if err != nil {
		log.Fatal().Err(err).Msg("Failed to connect to DB")
	}
	defer db.Close()

	rdb = redis.NewClient(&redis.Options{Addr: "localhost:6379"})

	app := fiber.New(fiber.Config{DisableStartupMessage: true})

	app.Use(requestid.New())
	app.Use(func(c *fiber.Ctx) error {
		log.Info().
			Str("request_id", c.Get("X-Request-ID")).
			Str("method", c.Method()).
			Str("path", c.Path()).
			Msg("Request")
		return c.Next()
	})

	app.Post("/login", loginHandler)
	app.Post("/users", createUser)
	app.Get("/users/:id", authMiddleware, rateLimitMiddleware, getUser)

	log.Info().Msg("Go Fiber starting on :8001")
	log.Fatal().Err(app.Listen(":8001"))
}

func authMiddleware(c *fiber.Ctx) error {
	auth := c.Get("Authorization")
	if !strings.HasPrefix(auth, "Bearer ") {
		return c.Status(401).JSON(fiber.Map{"error": "Missing or invalid token"})
	}
	tokenStr := strings.TrimPrefix(auth, "Bearer ")

	token, err := jwt.ParseWithClaims(tokenStr, &Claims{}, func(t *jwt.Token) (interface{}, error) {
		return secretKey, nil
	})
	if err != nil || !token.Valid {
		return c.Status(401).JSON(fiber.Map{"error": "Invalid token"})
	}
	c.Locals("user_id", token.Claims.(*Claims).Sub)
	return c.Next()
}

func rateLimitMiddleware(c *fiber.Ctx) error {
	userID := c.Locals("user_id").(string)
	key := "rate:" + userID

	count, err := rdb.Incr(context.Background(), key).Result()
	if err != nil {
		return c.Status(500).JSON(fiber.Map{"error": "Rate limit error"})
	}
	if count == 1 {
		rdb.Expire(context.Background(), key, time.Duration(rateWindowSec)*time.Second)
	}
	if count > int64(rateLimit) {
		return c.Status(429).JSON(fiber.Map{"error": "Rate limit exceeded"})
	}
	return c.Next()
}

func loginHandler(c *fiber.Ctx) error {
	token := jwt.NewWithClaims(jwt.SigningMethodHS256, Claims{
		Sub: "benchmark-user",
		RegisteredClaims: jwt.RegisteredClaims{
			IssuedAt: jwt.NewNumericDate(time.Now()),
		},
	})
	signed, _ := token.SignedString(secretKey)
	return c.JSON(fiber.Map{"access_token": signed, "token_type": "bearer"})
}

func createUser(c *fiber.Ctx) error {
	var u struct {
		Name  string `json:"name"`
		Email string `json:"email"`
	}
	if err := c.BodyParser(&u); err != nil {
		return c.Status(400).JSON(fiber.Map{"error": "Invalid body"})
	}

	var id int
	err := db.QueryRow("INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id", u.Name, u.Email).Scan(&id)
	if err != nil {
		return c.Status(500).JSON(fiber.Map{"error": err.Error()})
	}
	return c.JSON(fiber.Map{"id": id, "name": u.Name, "email": u.Email})
}

func getUser(c *fiber.Ctx) error {
	idStr := c.Params("id")
	id, err := strconv.Atoi(idStr)
	if err != nil {
		return c.Status(400).JSON(fiber.Map{"error": "Invalid ID"})
	}

	var u User
	err = db.QueryRow("SELECT id, name, email FROM users WHERE id = $1", id).Scan(&u.ID, &u.Name, &u.Email)
	if err == sql.ErrNoRows {
		return c.Status(404).JSON(fiber.Map{"error": "User not found"})
	}
	if err != nil {
		return c.Status(500).JSON(fiber.Map{"error": err.Error()})
	}
	return c.JSON(u)
}
