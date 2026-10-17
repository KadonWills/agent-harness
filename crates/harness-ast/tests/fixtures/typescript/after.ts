import { db } from "./db";
import axios from "axios";

export interface User {
  id: string;
  emailAddress: string;
}

export function getUser(id: string, includeDeleted: boolean): User {
  return db.find(id, includeDeleted);
}

export function displayName(user: User) {
  return user.id.toUpperCase();
}

export const sum = (a: number, b: number) => a + b;

export class Repo {
  constructor() {
    this.store = new Map();
  }

  save(user: User) {
    return db.save(user);
  }

  async fetchRemote() {
    return axios.get("/u");
  }
}

// Agents produce slightly broken code; the rest must still be understood.
function half(x: number {
  return x / 2;
