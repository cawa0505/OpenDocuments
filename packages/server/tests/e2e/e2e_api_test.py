#!/usr/bin/env python3
import json
import urllib.request
import urllib.error
import sys
import time

BASE_URL = "http://192.168.77.200:3006"
TEST_WORKSPACE = "E2E_Test_Workspace"

def log_test(name, success, message=""):
    status = "🟢 SUCCESS" if success else "🔴 FAILED"
    print(f"[{status}] - {name} {message}")
    if not success:
        sys.exit(1)

def request_json(path, method="GET", data=None, headers=None):
    url = f"{BASE_URL}{path}"
    req_headers = {
        "Content-Type": "application/json",
        "X-Workspace": TEST_WORKSPACE
    }
    if headers:
        req_headers.update(headers)
    
    body = json.dumps(data).encode("utf-8") if data is not None else None
    req = urllib.request.Request(url, data=body, headers=req_headers, method=method)
    
    try:
        with urllib.request.urlopen(req, timeout=15) as res:
            res_data = res.read().decode("utf-8")
            if res.headers.get("Content-Type", "").startswith("application/json"):
                return res.status, json.loads(res_data)
            return res.status, res_data
    except urllib.error.HTTPError as e:
        try:
            err_data = e.read().decode("utf-8")
            return e.code, json.loads(err_data)
        except Exception:
            return e.code, f"HTTP Error {e.code}"
    except Exception as e:
        return 500, str(e)

def test_healthz():
    code, data = request_json("/api/v1/healthz")
    log_test("GET /api/v1/healthz", code == 200, f"(Code: {code})")

def test_dictionary_crud():
    print("\n--- Starting Glossary Dictionary CRUD Test ---")
    # 1. Create a dictionary term via POST (upsert)
    term_data = {"key": "Harness Engineering", "value": "測試與驗證工程學"}
    code, data_raw = request_json("/api/v1/dictionary", method="POST", data=term_data)
    is_dict = isinstance(data_raw, dict)
    data_dict = data_raw if is_dict else {}
    log_test("POST /api/v1/dictionary (Create)", code == 201 and is_dict and data_dict.get("key") == "Harness Engineering", f"Response: {data_raw}")
    
    term_id = data_dict.get("id") if is_dict else None
    
    # 2. Get terms list and check "entries" wrapper
    code, list_raw = request_json("/api/v1/dictionary")
    is_list_dict = isinstance(list_raw, dict)
    list_dict = list_raw if is_list_dict else {}
    entries = list_dict.get("entries", []) if is_list_dict else []
    log_test("GET /api/v1/dictionary (List)", code == 200 and isinstance(entries, list) and len(entries) > 0, f"Found {len(entries)} terms")
    
    # 3. Update the term via POST (upsert)
    update_data = {"key": "Harness Engineering", "value": "高強度安全防禦工程學"}
    code, upd_raw = request_json("/api/v1/dictionary", method="POST", data=update_data)
    is_dict_upd = isinstance(upd_raw, dict)
    upd_dict = upd_raw if is_dict_upd else {}
    log_test("POST /api/v1/dictionary (Update/Upsert)", code == 201 and is_dict_upd and upd_dict.get("value") == "高強度安全防禦工程學", f"Response: {upd_raw}")
    
    # 4. Delete the term
    code, del_raw = request_json(f"/api/v1/dictionary/{term_id}", method="DELETE")
    is_dict_del = isinstance(del_raw, dict)
    del_dict = del_raw if is_dict_del else {}
    log_test("DELETE /api/v1/dictionary/:id (Delete)", code == 200 and is_dict_del and del_dict.get("deleted") is True, f"Response: {del_raw}")

def test_chat_stream_and_patch():
    print("\n--- Starting Chat Stream and Patch Test ---")
    url = f"{BASE_URL}/api/v1/chat/stream"
    query_data = {
        "query": "What is the capital of France?",
        "profile": "fast"
    }
    req_headers = {
        "Content-Type": "application/json",
        "X-Workspace": TEST_WORKSPACE
    }
    
    body = json.dumps(query_data).encode("utf-8")
    req = urllib.request.Request(url, data=body, headers=req_headers, method="POST")
    
    stream_success = False
    conversation_id = None
    chunks_received = []
    
    print("[..] Streaming chat connection initiated...")
    try:
        with urllib.request.urlopen(req, timeout=30) as res:
            log_test("POST /api/v1/chat/stream connection", res.status == 200, f"(Status: {res.status})")
            
            # Read SSE stream chunk by chunk
            for line in res:
                line_str = line.decode("utf-8").strip()
                if not line_str:
                    continue
                
                if line_str.startswith("event:"):
                    event_type = line_str.replace("event:", "").strip()
                    if event_type == "error":
                        log_test("Stream Event Check", False, "Received error event in stream!")
                        
                elif line_str.startswith("data:"):
                    data_str = line_str.replace("data:", "").strip()
                    try:
                        parsed = json.loads(data_str)
                        if isinstance(parsed, dict) and "conversationId" in parsed:
                            conversation_id = parsed["conversationId"]
                        if isinstance(parsed, str):
                            chunks_received.append(parsed)
                    except Exception:
                        pass
            
            print(f"[ok] Full answer chunks length: {len(chunks_received)}")
            log_test("Stream Chat Result Check", len(chunks_received) > 0 or conversation_id is not None, f"Chunks count: {len(chunks_received)}, Conversation ID: {conversation_id}")
            stream_success = True
    except Exception as e:
        log_test("Stream Chat Connection", False, f"Exception: {str(e)}")

    if stream_success and conversation_id:
        print(f"\n--- Testing PATCH Conversation Title for ID {conversation_id} ---")
        patch_data = {"title": "Paris Conversation E2E"}
        code, data = request_json(f"/api/v1/conversations/{conversation_id}", method="PATCH", data=patch_data)
        log_test("PATCH /api/v1/conversations/:id", code == 200, f"Code: {code}, Response: {data}")
        
        # Cleanup the test conversation
        code, data = request_json(f"/api/v1/conversations/{conversation_id}", method="DELETE")
        log_test("DELETE /api/v1/conversations/:id (Cleanup)", code == 200, f"Code: {code}, Response: {data}")

if __name__ == "__main__":
    print("==================================================")
    print("🚀 Running OpenDocuments E2E Integration Test Suite")
    print("==================================================")
    try:
        test_healthz()
        test_dictionary_crud()
        test_chat_stream_and_patch()
        print("\n==================================================")
        print("🏆 ALL INTEGRATION TESTS PASSED SUCCESSFULLY! 🎉")
        print("==================================================")
    except KeyboardInterrupt:
        print("\nTest cancelled.")
        sys.exit(1)
