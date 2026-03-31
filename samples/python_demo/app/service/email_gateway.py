class EmailGateway:
    def deliver(self, user_id: str, message: str) -> None:
        print(f"EMAIL to {user_id}: {message}")
