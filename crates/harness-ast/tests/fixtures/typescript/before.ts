import { db } from "./db";

export interface User {
  id: string;
  email: string;
}

export function getUser(id: string): User {
  return db.find(id);
}

export function formatName(user: User) {
  return user.id.toUpperCase();
}

export const sum = (a: number, b: number) => a + b;

export class Repo {
  constructor() {
    this.cache = new Map();
  }

  save(user: User) {
    return db.save(user);
  }
}
