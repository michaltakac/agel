import java.nio.file.Files;
import java.nio.file.Paths;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.PreparedStatement;

public class SetupAccount {
    public static void main(String[] args) throws Exception {
        if (args.length != 0) throw new IllegalArgumentException("this command grants only agelsetup");
        String port = Files.readAllLines(Paths.get("/world/.data/postgres/postmaster.pid")).get(3).trim();
        if (!port.matches("[0-9]+")) throw new IllegalStateException("invalid PostgreSQL port");
        try (Connection db = DriverManager.getConnection("jdbc:postgresql://127.0.0.1:" + port + "/postgres", "postgres", "");
             PreparedStatement sql = db.prepareStatement("UPDATE public.accounts SET rights='ADMINISTRATOR' WHERE account_name='agelsetup'")) {
            if (sql.executeUpdate() != 1) throw new IllegalStateException("create the dedicated agelsetup account first");
        }
        System.out.println("Granted admin to dedicated setup account agelsetup; experimental accounts unchanged.");
    }
}
