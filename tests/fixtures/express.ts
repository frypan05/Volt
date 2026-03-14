import express from 'express';

const app = express();

app.get('/users/:id', handler);
app.post('/users', createUser);
