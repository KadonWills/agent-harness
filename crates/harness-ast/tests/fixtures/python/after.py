import os
import requests
from dataclasses import dataclass


@dataclass
class User:
    id: str
    email_address: str

    def display(self):
        return self.email_address


class Store:
    def __init__(self, path):
        self.path = path

    def load(self, key, default=None):
        return default


def resolve_user(store, user_id):
    return store.load(user_id)


def double(x):
    return x * 2


def fetch(url):
    return requests.get(url)
