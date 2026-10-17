const express = require("express");
import { render } from "./view.js";

export function handler(req, res) {
  res.send(render(req.query));
}

export class Cache {
  get(key) {
    return this.map.get(key);
  }
}
