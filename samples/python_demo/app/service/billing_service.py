from app.model.user import User
from app.service.user_repository import UserRepository
from app.util.logger_util import LoggerUtil


class BillingService:
    def __init__(self) -> None:
        self._user_repository = UserRepository()

    def charge_user(self, user_id: str, amount: float) -> bool:
        LoggerUtil.info(f"Charge requested for {user_id} amount {amount}")
        user: User | None = self._user_repository.find_by_id(user_id)
        if user is None:
            LoggerUtil.error(f"User not found: {user_id}")
            return False
        return user.credit(amount)
