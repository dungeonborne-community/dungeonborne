// Decompile functions at given addresses and their callees up to a depth.
// args: <outfile> <depth> <addr>[@depth] ...
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.app.decompiler.*;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import java.io.*;
import java.util.*;

public class DecompTree extends GhidraScript {
    PrintWriter out;
    DecompInterface di;
    Set<Address> seen = new HashSet<>();

    Function ensure(Address a) throws Exception {
        Function f = getFunctionAt(a);
        if (f != null) return f;
        new DisassembleCommand(a, null, true).applyTo(currentProgram, monitor);
        f = createFunction(a, null);
        return f;
    }

    void walk(Address a, int depth, int maxCallees) throws Exception {
        if (!seen.add(a)) return;
        Function f = ensure(a);
        if (f == null) { out.println("// no function at " + a); return; }
        DecompileResults r = di.decompileFunction(f, 120, monitor);
        out.println("\n// ===== " + f.getName() + " @ " + a + " (depth " + depth + ")");
        out.println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "// decompile failed: " + r.getErrorMessage());
        out.flush();
        if (depth <= 0) return;
        List<Address> callees = new ArrayList<>();
        for (Instruction ins : currentProgram.getListing().getInstructions(f.getBody(), true)) {
            if (!ins.getFlowType().isCall()) continue;
            for (Address t : ins.getFlows()) if (!callees.contains(t)) callees.add(t);
        }
        int n = 0;
        for (Address t : callees) { if (n++ >= maxCallees) break; walk(t, depth - 1, maxCallees); }
    }

    public void run() throws Exception {
        String[] args = getScriptArgs();
        out = new PrintWriter(new FileWriter(args[0]));
        int defDepth = Integer.parseInt(args[1]);
        di = new DecompInterface();
        di.openProgram(currentProgram);
        for (int i = 2; i < args.length; i++) {
            String[] p = args[i].split("@");
            int d = p.length > 1 ? Integer.parseInt(p[1]) : defDepth;
            walk(toAddr(p[0]), d, 25);
        }
        out.close();
    }
}
