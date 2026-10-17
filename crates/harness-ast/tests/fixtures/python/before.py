import os
from dataclasses import dataclass


@dataclass
class User:
    id: str
    email: str

    def display(self):
        return self.email


class Store:
    def __init__(self, path):
        self.path = path
        self.cache = {}

    def load(self, key):
        return self.cache.get(key)


def resolve_user(store, user_id):
    return store.load(user_id)


def legacy_helper(x):
    return x * 2
