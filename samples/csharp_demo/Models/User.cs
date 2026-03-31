namespace Demo.Models;

public class User
{
    public string Id { get; }
    public decimal Balance { get; private set; }

    public User(string id, decimal balance)
    {
        Id = id;
        Balance = balance;
    }

    public bool Credit(decimal amount)
    {
        if (amount <= Balance)
        {
            Balance -= amount;
            return true;
        }
        return false;
    }
}
