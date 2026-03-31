from app.model.user import User


class UserRepository:
    def find_by_id(self, user_id: str) -> User | None:
        if user_id == "u-1001":
            return User(user_id, 150.0)
        return None
