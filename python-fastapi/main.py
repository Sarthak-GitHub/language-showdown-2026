import time
from typing import Dict
from fastapi import FastAPI, Depends, HTTPException, status, Request, Header
from fastapi.security import HTTPBearer, HTTPAuthorizationCredentials
from pydantic import BaseModel
import sqlalchemy as sa
from sqlalchemy.ext.asyncio import create_async_engine, AsyncSession
from sqlalchemy.orm import sessionmaker
import redis.asyncio as redis
import jwt
import structlog
import orjson

app = FastAPI(default_response_class=orjson_response.ORJSONResponse)

# Config
DATABASE_URL = "postgresql+asyncpg://benchmark:benchmark@localhost:5432/benchmark"
SECRET_KEY = "super-secret-key-for-benchmark-only"
ALGORITHM = "HS256"
RATE_LIMIT = 100  # req per minute
RATE_WINDOW = 60

engine = create_async_engine(DATABASE_URL, echo=False)
async_session = sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)

redis_client = redis.Redis(host='localhost', port=6379, decode_responses=True)

security = HTTPBearer()
logger = structlog.get_logger()

class UserCreate(BaseModel):
    name: str
    email: str

class UserOut(BaseModel):
    id: int
    name: str
    email: str

async def get_db():
    async with async_session() as session:
        yield session

async def rate_limit_check(authorization: HTTPAuthorizationCredentials = Depends(security)):
    token = authorization.credentials
    try:
        payload = jwt.decode(token, SECRET_KEY, algorithms=[ALGORITHM])
        user_id = payload.get("sub", "anonymous")
    except:
        raise HTTPException(status_code=401, detail="Invalid token")

    key = f"rate:{user_id}"
    count = await redis_client.incr(key)
    if count == 1:
        await redis_client.expire(key, RATE_WINDOW)
    if count > RATE_LIMIT:
        raise HTTPException(status_code=429, detail="Rate limit exceeded")
    return user_id

@app.post("/users", response_model=UserOut)
async def create_user(user: UserCreate, db: AsyncSession = Depends(get_db)):
    query = sa.text("INSERT INTO users (name, email) VALUES (:name, :email) RETURNING id, name, email")
    result = await db.execute(query, {"name": user.name, "email": user.email})
    await db.commit()
    row = result.fetchone()
    return UserOut(id=row[0], name=row[1], email=row[2])

@app.get("/users/{user_id}", response_model=UserOut)
async def get_user(user_id: int, _=Depends(rate_limit_check), db: AsyncSession = Depends(get_db)):
    query = sa.text("SELECT id, name, email FROM users WHERE id = :id")
    result = await db.execute(query, {"id": user_id})
    row = result.fetchone()
    if not row:
        raise HTTPException(status_code=404, detail="User not found")
    return UserOut(id=row[0], name=row[1], email=row[2])

# Dummy login for benchmark (always returns same token)
@app.post("/login")
async def login():
    token = jwt.encode({"sub": "benchmark-user", "iat": int(time.time())}, SECRET_KEY, algorithm=ALGORITHM)
    return {"access_token": token, "token_type": "bearer"}

# PUT and DELETE omitted for brevity — add similarly if needed
