import java.nio.file.Files;
import java.nio.file.Paths;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.PreparedStatement;

public class SetupAccount {
    public static void main(String[] args) throws Exception {
        String account = "agelsetup";
        String rights = "ADMINISTRATOR";
        if (args.length != 0) {
            if (args.length != 1 || !(args[0].equals("craft-grant") || args[0].equals("craft-revoke"))) {
                throw new IllegalArgumentException("expected no argument, craft-grant or craft-revoke");
            }
            account = "agelcraft";
            if (args[0].equals("craft-revoke")) rights = "";
        }
        String port = Files.readAllLines(Paths.get("/world/.data/postgres/postmaster.pid")).get(3).trim();
        if (!port.matches("[0-9]+")) throw new IllegalStateException("invalid PostgreSQL port");
        try (Connection db = DriverManager.getConnection("jdbc:postgresql://127.0.0.1:" + port + "/postgres", "postgres", "");
             PreparedStatement sql = db.prepareStatement("UPDATE public.accounts SET rights=? WHERE account_name=?")) {
            sql.setString(1, rights);
            sql.setString(2, account);
            if (sql.executeUpdate() != 1) throw new IllegalStateException("create the dedicated fixture account first");
        }
        if (args.length == 0) {
            System.out.println("Granted admin to dedicated setup account agelsetup; experimental accounts unchanged.");
        } else {
            System.out.println("Updated rights for dedicated fixture " + account);
        }
    }
}
