using Demo.Models;

namespace Demo.Services;

public class UserRepository
{
    public User? FindById(string userId)
    {
        if (userId == "u-1001")
        {
            return new User(userId, 150.0m);
        }
        return null;
    }
}
