CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    email VARCHAR(150) UNIQUE NOT NULL
);

-- For benchmark insert some dummy data
INSERT INTO users (name, email) VALUES
    ('Benchmark User 1', 'bench1@example.com'),
    ('Benchmark User 2', 'bench2@example.com')
ON CONFLICT DO NOTHING;
