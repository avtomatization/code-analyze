using Demo.Models;
using Demo.Utils;

namespace Demo.Services;

public class InvoiceService
{
    private readonly UserRepository _userRepository = new();

    public bool ChargeUser(string userId, decimal amount)
    {
        Log.WriteInfo($"Charge requested for {userId} amount {amount}");
        User? user = _userRepository.FindById(userId);
        if (user is null)
        {
            Log.WriteError($"User not found: {userId}");
            return false;
        }

        return user.Credit(amount);
    }
}
