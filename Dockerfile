FROM node:25-slim

WORKDIR /app

ENV NODE_ENV=production
ENV HOST=0.0.0.0
ENV PORT=4173
ENV JOB_STORE_PATH=/app/data/jobs.json
ENV ALLOW_SERVER_PATH_SCAN=0
ENV STORE_FULL_MANIFESTS=0

COPY package.json README.md submission-readme.txt ./
COPY src ./src
COPY web ./web
COPY samples ./samples
COPY test ./test
COPY cli ./cli

RUN mkdir -p /app/data

EXPOSE 4173

CMD ["npm", "run", "dev"]
