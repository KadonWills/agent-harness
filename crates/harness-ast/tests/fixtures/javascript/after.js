const express = require("express");
import { render } from "./view.js";
import lodash from "lodash";

export function handler(req, res, next) {
  res.send(render(lodash.pick(req.query, ["q"])));
}

export class Cache {
  get(key) {
    return this.map.get(key);
  }

  clear() {
    this.map = new Map();
  }
}

export const isEmpty = (value) => value == null;
