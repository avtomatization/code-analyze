class User:
    def __init__(self, user_id: str, balance: float) -> None:
        self.id = user_id
        self.balance = balance

    def credit(self, amount: float) -> bool:
        if amount <= self.balance:
            self.balance -= amount
            return True
        return False
