import json
import nn_connector

conn = nn_connector.PyFuzzActiveConnector("nn_v0.1", 7878)

while (True):
    try:
        entry = conn.recv_input()
        # entry['input'] - initial input
        # entry['map'] - original map
        
        # Require to send heatmap back
        conn.send_heatmap([12,45])
        
        
        ht = conn.recv_map()
        # entry['input'] - initial input (only with debug_messages feature in nn_fuzz)
        # entry['map'] - original map
        
        # throws StopIteration on last map from fuzzer
        
        
    except TimeoutError as timeout_msg:
        print(timeout_msg)
    except RuntimeError as re:
        print(re)
        break
    
    
while(True):
    pass
    
print("End cycle")