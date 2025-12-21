#!/usr/bin/env python3
"""
Test script for UCI pondering chains in z-slon
Tests the sequence: bestmove+ponder -> ponderhit -> bestmove+new_ponder
"""

import subprocess
import sys
import time
import re

def test_ponder_chain():
    """Test pondering chain functionality"""
    print("Testing Pondering Chains in z-slon")
    print("=" * 50)
    
    # Start the engine
    try:
        engine = subprocess.Popen(
            ['./target/release/z-slon'],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1
        )
    except FileNotFoundError:
        print("Error: Engine not found. Please run 'cargo build --release' first.")
        return False
    
    def send_command(cmd):
        """Send command to engine and get response"""
        print(f"> {cmd}")
        engine.stdin.write(cmd + '\n')
        engine.stdin.flush()
        
        # Read response
        response = []
        start_time = time.time()
        while time.time() - start_time < 5:  # 5 second timeout
            line = engine.stdout.readline().strip()
            if line:
                print(f"< {line}")
                response.append(line)
                if 'bestmove' in line:
                    break
        return response
    
    # Test sequence
    try:
        # Initialize
        print("\n1. Initializing UCI...")
        response = send_command('uci')
        time.sleep(0.5)
        
        send_command('isready')
        time.sleep(0.5)
        
        # Start a ponder search
        print("\n2. Starting ponder search...")
        send_command('ucinewgame')
        send_command('position startpos')
        
        # Capture ponder output
        print("\n3. Capturing ponder output...")
        response = send_command('go ponder')
        
        # Wait for and capture bestmove + ponder
        time.sleep(3)  # Give time for ponder search
        
        # Check if we got bestmove + ponder
        found_ponder = False
        bestmove_line = ''
        for line in response:
            if 'bestmove' in line and 'ponder' in line:
                found_ponder = True
                bestmove_line = line
                print(f"\n🎯 Found bestmove+ponder: {line}")
                break
        
        if not found_ponder:
            print("\n❌ Failed to get bestmove+ponder in initial search")
            engine.terminate()
            return False
        
        # Extract the ponder move
        match = re.search(r'bestmove (\S+)(?: ponder (\S+))?', bestmove_line)
        if match:
            bestmove = match.group(1)
            ponder_move = match.group(2)
            print(f"Best move: {bestmove}")
            print(f"Ponder move: {ponder_move}")
            
            if not ponder_move:
                print("❌ No ponder move found!")
                engine.terminate()
                return False
        
        print("\n✅ Step 1 passed: Engine outputs bestmove + ponder")
        
        # Now apply the best move and send ponderhit
        print(f"\n4. Applying best move {bestmove} and sending ponderhit...")
        send_command(f'position startpos moves {bestmove}')
        time.sleep(0.5)
        
        # Send ponderhit (this should trigger a new search with new ponder)
        print("\n5. Sending ponderhit...")
        response = send_command('ponderhit')
        time.sleep(3)  # Give time for new search
        
        # Check for new bestmove + ponder
        found_new_ponder = False
        for line in response:
            if 'bestmove' in line and 'ponder' in line:
                found_new_ponder = True
                print(f"\n🎯 Found new bestmove+ponder after ponderhit: {line}")
                break
        
        if found_new_ponder:
            print("\n✅ SUCCESS: Pondering chain is working!")
            print("Chain: bestmove+ponder -> ponderhit -> bestmove+new_ponder")
            result = True
        else:
            print("\n❌ FAILURE: No new ponder after ponderhit")
            print("This means the pondering chain is broken")
            result = False
        
        # Cleanup
        send_command('quit')
        engine.terminate()
        
        return result
        
    except Exception as e:
        print(f"\n❌ Error during test: {e}")
        engine.terminate()
        return False

if __name__ == '__main__':
    success = test_ponder_chain()
    sys.exit(0 if success else 1)
