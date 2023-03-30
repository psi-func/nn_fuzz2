import json
import nn_connector

conn = nn_connector.PyFuzzConnector(7878)

while (True):
    try:
        entry = conn.recv_input()
        with open("sample.json", "w") as file:
           json.dump(entry, file, ensure_ascii=False)
        
        break
    except TimeoutError as timeout_msg:
        print(timeout_msg)
    except RuntimeError as re:
        print(re)
        break
    
    
while(True):
    pass
    
print("End cycle")

