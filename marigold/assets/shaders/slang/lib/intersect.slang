public struct Ray {
    public float3 pos;
    public float3 dir;

    public Hit intersect<T>(T object)
        where T: Hittable
    {
        return object.getHit(this);
    }
}

public struct Hit {
    // material info
    public float3 position;
    public float distance;
    public float3 normal;
    public bool frontFace;
    public float2 uv;
    
    public bool success;

    public static const Hit NO_HIT = Hit();

    private __init() {
        success = false;
        position = float3(0.0);
        distance = 0.0;
        normal = float3(0.0);
        frontFace = false;
        uv = float2(0.0);
    }

    public __init(float3 position, float distance, float3 normal, bool frontFace, float2 uv) {
        success = true;
        this.position = position;
        this.distance = distance;
        this.normal = normal;
        this.frontFace = frontFace;
        this.uv = uv;
    }

    public float3 biasPositionPositive() {
        return this.position + this.normal * 0.001;
    }

    public float3 biasPositionNegative() {
        return this.position - this.normal * 0.001;
    }

    public static Hit merge(Hit a, Hit b) {
        Hit hit;

        // 1
        // if (a.success && b.success) {
        //     if (a.distance <= b.distance) {
        //         hit = a;
        //     } else {
        //         hit = b;
        //     }
        // } else if (a.success && !b.success) {
        //     hit = a;
        // } else if (b.success && !a.success) {
        //     hit = b;
        // }

        // 2
        bool useA = 
            select(a.success, a.distance, float.maxValue) < 
            select(b.success, b.distance, float.maxValue);

        hit = select(useA, a, b);
        hit.success = a.success || b.success;

        return hit;
    }
}

public interface Hittable {
    Hit getHit(Ray ray);
}
