#!/user/bin/env python

import os
import subprocess
from flask import Flask, request, jsonify
from minio import Minio
from minio.error import S3Error
import plot.plot as plot

app = Flask(__name__)

# MinIO client setup
minio_client = Minio(
    endpoint=os.environ["MINIO_SERVER_URL"],  # e.g., "localhost:9000"
    access_key=os.environ["MINIO_ROOT_USER"],
    secret_key=os.environ["MINIO_ROOT_PASSWORD"],
    secure=False  # Change to True if using HTTPS
)

# Filenames for stats CSV and plot files
LOCAL_CSV_FILENAME = "data.csv"
LOCAL_PLOT_FILENAME = "plot.svg"
CLOUD_CSV_FILENAME = "data.csv"
CLOUD_PLOT_FILENAME = "plot.svg"

def update_csv(nyt_token, start_date):
    try:
        subprocess.run(
            ["crossword", "-t", nyt_token, "-s", start_date, LOCAL_CSV_FILENAME],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except subprocess.CalledProcessError as e:
        print(f"Error in Crossword app: {e}")
        raise

def generate_plot():
    plot.generate(LOCAL_CSV_FILENAME, LOCAL_PLOT_FILENAME)

def upload_file_to_minio(bucket_name, local_file_path, object_name):
    try:
        if not minio_client.bucket_exists(bucket_name):
            minio_client.make_bucket(bucket_name)

        minio_client.fput_object(bucket_name, object_name, local_file_path)
        print(f"Successfully uploaded {object_name} to bucket {bucket_name}.")
    except S3Error as e:
        print(f"Error uploading to MinIO: {e}")

@app.route("/", methods=["POST"])
def update_database_and_plot():
    data = request.json
    nyt_token = data.get("nyt_token")
    start_date = data.get("start_date")  

    if not nyt_token:
        return jsonify({"error": "NYT token is required"}), 400
    
    if not start_date:
        return jsonify({"error": "Start Date is required"}), 400

    try:
        update_csv(nyt_token, start_date)
        generate_plot()

        # Replace with your MinIO bucket name
        bucket_name = "your-minio-bucket-name"
        upload_file_to_minio(bucket_name, LOCAL_CSV_FILENAME, CLOUD_CSV_FILENAME)
        upload_file_to_minio(bucket_name, LOCAL_PLOT_FILENAME, CLOUD_PLOT_FILENAME)

        return jsonify({"message": "Success!"}), 200

    except Exception as e:
        return jsonify({"error": str(e)}), 500

@app.route("/healthz")
def healthz():
    return "Up"

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=int(os.environ.get("PORT", 8080)))

